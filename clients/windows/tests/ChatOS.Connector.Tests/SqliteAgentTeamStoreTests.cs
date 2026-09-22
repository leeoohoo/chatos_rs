using ChatOS.Connector.AgentTeams;
using ChatOS.Connector.Persistence;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class SqliteAgentTeamStoreTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(), "chatos-agent-team-tests", Guid.NewGuid().ToString("N"));
    private SqliteAgentTeamStore _store = null!;

    public async Task InitializeAsync()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        _store = new SqliteAgentTeamStore(database);
    }

    public Task DisposeAsync()
    {
        if (Directory.Exists(_directory)) Directory.Delete(_directory, recursive: true);
        return Task.CompletedTask;
    }

    [Fact]
    public async Task ProfilesRoomsAndDirectChatsAreDurableAndAccountScoped()
    {
        var manager = await CreateAgentAsync("alice", "经理");
        var worker = await CreateAgentAsync("alice", "开发");
        var room = await _store.CreateRoomAsync(
            "alice", "project-1", new("交付团队", "完成 Windows 客户端"), manager.Id);
        await _store.UpsertMemberAsync("alice", room.Id, worker.Id,
            new("developer", "实现功能"));
        var configured = await _store.UpdateRoomAsync("alice", room.Id, room.Draft,
            worker.Id, manager.Id, AgentRoomStatus.Active);
        var direct = await _store.OpenHumanAgentDirectAsync("alice", worker.Id);
        var reopened = await _store.OpenHumanAgentDirectAsync("alice", worker.Id);
        var agentDirect = await _store.OpenAgentDirectAsync("alice", manager.Id, worker.Id);
        var reversedAgentDirect = await _store.OpenAgentDirectAsync("alice", worker.Id, manager.Id);

        Assert.Equal(worker.Id, configured.DefaultAgentId);
        Assert.Equal(direct.Id, reopened.Id);
        Assert.Equal(agentDirect.Id, reversedAgentDirect.Id);
        Assert.Equal(AgentConversationKind.AgentAgentDirect, agentDirect.Kind);
        Assert.Equal(2, (await _store.ListMembersAsync("alice", agentDirect.Id)).Count);
        Assert.Equal(2, (await _store.ListMembersAsync("alice", room.Id)).Count);
        Assert.Single(await _store.ListRoomsAsync("alice", "project-1"));
        Assert.Equal(2, (await _store.ListRoomsAsync("alice", "direct")).Count);
        Assert.Empty(await _store.ListAgentsAsync("bob"));
        Assert.Empty(await _store.ListRoomsAsync("bob", "project-1"));
    }

    [Fact]
    public async Task TeamCreationEnqueuesOneDurableAssetMaintenanceDelivery()
    {
        var manager = await CreateAgentAsync("alice", "经理");
        var room = await _store.CreateRoomAsync(
            "alice", "project-1", new("产品团队", "交付产品"), manager.Id);

        var delivery = Assert.IsType<AgentDelivery>(
            await _store.ClaimNextDeliveryAsync("alice"));
        Assert.Equal(manager.Id, delivery.TargetAgentId);
        Assert.Equal($"team-asset-maintenance:{room.Id}:v1", delivery.DeduplicationKey);
        Assert.Equal(AgentDeliveryTrigger.Mention, delivery.Trigger);
        var message = Assert.Single(await _store.ListMessagesAsync("alice", room.Id));
        Assert.Equal(AgentMessageSenderKind.System, message.SenderKind);
        Assert.Contains("项目概览", message.Content, StringComparison.Ordinal);
        await _store.CompleteDeliveryAsync("alice", delivery.Id, null);

        _ = await _store.UpdateRoomAsync("alice", room.Id, room.Draft,
            manager.Id, manager.Id, AgentRoomStatus.Active);
        Assert.Null(await _store.ClaimNextDeliveryAsync("alice"));
    }

    [Fact]
    public async Task MessagesPersistAttachmentsAndRouteToMentionOrDefaultAgent()
    {
        var (manager, worker, room) = await CreateConfiguredTeamAsync();
        var defaultPost = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "开始工作"));
        var attachment = new AgentMessageAttachment("attachment-1", "plan.md", "text/markdown",
            AgentMessageAttachmentKind.File, 4, "plan"u8.ToArray());
        var mentionPost = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "请检查", [worker.Id], [attachment]));

        Assert.Equal(AgentDeliveryTrigger.DefaultAgent, Assert.Single(defaultPost.Deliveries).Trigger);
        Assert.Equal(worker.Id, Assert.Single(mentionPost.Deliveries).TargetAgentId);
        var stored = Assert.Single(await _store.ListMessagesAsync("alice", room.Id,
                includeAttachmentPayloads: true),
            value => value.Id == mentionPost.Message.Id);
        Assert.Equal(worker.Id, Assert.Single(stored.MentionedAgentIds));
        Assert.Equal("plan", System.Text.Encoding.UTF8.GetString(Assert.Single(stored.Attachments).Data));
        var metadata = Assert.Single(await _store.ListMessagesAsync("alice", room.Id),
            value => value.Id == mentionPost.Message.Id);
        Assert.Empty(Assert.Single(metadata.Attachments).Data);
        var downloaded = await _store.GetMessageAttachmentAsync(
            "alice", room.Id, attachment.Id);
        Assert.Equal("plan", System.Text.Encoding.UTF8.GetString(downloaded!.Data));
        var managerMember = Assert.Single(await _store.ListMembersAsync("alice", room.Id),
            value => value.AgentId == manager.Id);
        var attachmentReferences = new AgentRunReferenceVault();
        var attachmentResult = await new AgentTeamToolExecutor(_store, null!).ExecuteAsync(
            manager, managerMember, room,
            new AgentDelivery("attachment-run", "alice", room.Id, mentionPost.Message.Id,
                mentionPost.Message.RootMessageId, manager.Id, AgentDeliveryTrigger.Mention,
                AgentDeliveryStatus.Running, 1, 0, "attachment-test", null, null, 1, null, 1),
            new AgentToolCall("read", "chat_read_attachment",
                $$"""{"attachment_ref":"{{attachmentReferences.AttachmentReference(room.Id, attachment.Id)}}","limit":2}"""),
            CancellationToken.None, attachmentReferences);
        using var attachmentDocument = System.Text.Json.JsonDocument.Parse(
            attachmentResult.Content);
        Assert.Equal("pl", attachmentDocument.RootElement.GetProperty("content").GetString());
        Assert.Equal(2, attachmentDocument.RootElement.GetProperty("next_offset").GetInt32());

        var owners = await _store.ListOwnersWithPendingDeliveriesAsync();
        Assert.Equal("alice", Assert.Single(owners));
        var claimed = Assert.IsType<AgentDelivery>(await _store.ClaimNextDeliveryAsync("alice"));
        Assert.Equal(AgentDeliveryStatus.Running, claimed.Status);
        Assert.Equal(1, claimed.Attempt);
        var completed = await _store.CompleteDeliveryAsync("alice", claimed.Id, null);
        Assert.Equal(AgentDeliveryStatus.Completed, completed.Status);
    }

    [Fact]
    public async Task AgentMentionsRespectHopAndRootRunLimits()
    {
        var (manager, worker, room) = await CreateConfiguredTeamAsync();
        var root = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "root", [manager.Id, worker.Id]));
        var stopped = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Agent, manager.Id, "too deep", [worker.Id],
                RootMessageId: root.Message.Id, HopCount: 4));

        Assert.Empty(stopped.Deliveries);
        Assert.Equal("maximum_hop_count", stopped.RoutingStopReason);

        for (var index = 0; index < 10; index++)
        {
            var sender = index % 2 == 0 ? manager : worker;
            var target = index % 2 == 0 ? worker : manager;
            var post = await _store.PostMessageAsync("alice", room.Id,
                new(AgentMessageSenderKind.Agent, sender.Id, $"turn {index}", [target.Id],
                    RootMessageId: root.Message.Id, HopCount: 1));
            Assert.Single(post.Deliveries);
        }

        var capped = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Agent, manager.Id, "capped", [worker.Id],
                RootMessageId: root.Message.Id, HopCount: 1));
        Assert.Empty(capped.Deliveries);
        Assert.Equal("maximum_agent_runs", capped.RoutingStopReason);
    }

    [Fact]
    public async Task TodoDependenciesReleaseOnlyAfterCompletionAndUseRevisions()
    {
        var (manager, worker, room) = await CreateConfiguredTeamAsync();
        var prerequisite = await _store.CreateTodoAsync("alice",
            new(room.Id, manager.Id, "设计", Priority: AgentTodoPriority.High));
        var dependent = await _store.CreateTodoAsync("alice",
            new(room.Id, worker.Id, "实现", DependencyIds: [prerequisite.Id]));

        Assert.Equal(AgentTodoStatus.Ready, prerequisite.Status);
        Assert.Equal(AgentTodoStatus.Pending, dependent.Status);
        var conflict = await Assert.ThrowsAsync<AgentTeamException>(() =>
            _store.UpdateTodoAsync("alice", dependent.Id, dependent.Revision,
                AgentTodoStatus.InProgress, string.Empty));
        Assert.Equal(AgentTeamError.Conflict, conflict.Code);

        var completed = await _store.UpdateTodoAsync("alice", prerequisite.Id,
            prerequisite.Revision, AgentTodoStatus.Completed, "设计完成");
        Assert.Equal(2, completed.Revision);
        dependent = Assert.Single(await _store.ListTodosAsync("alice", room.Id),
            value => value.Id == dependent.Id);
        Assert.Equal(AgentTodoStatus.Ready, dependent.Status);
        Assert.True(dependent.Revision > 1);
        await _store.AppendTodoProgressAsync("alice", dependent.Id, worker.Id,
            AgentTodoProgressKind.Update, "实现中", "完成核心逻辑",
        [
            new(AgentTeamAssetCategory.Decision, "运行策略", "# 策略\n串行调度",
                "实现中确认需要避免同账号并发执行"),
        ]);
        var progress = Assert.Single(await _store.ListTodoProgressAsync("alice", dependent.Id));
        var suggestion = Assert.Single(progress.Suggestions);
        Assert.Equal("运行策略", suggestion.Title);
        var triggers = new List<AgentDeliveryTrigger>();
        for (var index = 0; index < 3; index++)
        {
            var pendingDelivery = await _store.ClaimNextDeliveryAsync("alice");
            if (pendingDelivery is null) break;
            triggers.Add(pendingDelivery.Trigger);
            await _store.CompleteDeliveryAsync("alice", pendingDelivery.Id, null);
        }
        Assert.Contains(AgentDeliveryTrigger.TodoStatus, triggers);

        var stale = await Assert.ThrowsAsync<AgentTeamException>(() =>
            _store.UpdateTodoAsync("alice", prerequisite.Id, 1,
                AgentTodoStatus.Completed, "重复"));
        Assert.Equal(AgentTeamError.Conflict, stale.Code);
    }

    [Fact]
    public async Task TodoExecutionContractCapabilitiesAndSourcesAreDurableAndImmutable()
    {
        var (manager, worker, room) = await CreateConfiguredTeamAsync();
        var teamSource = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "实现 Windows 执行合同"));
        var direct = await _store.OpenHumanAgentDirectAsync("alice", manager.Id);
        var directSource = await _store.PostMessageAsync("alice", direct.Id,
            new(AgentMessageSenderKind.Human, null, "验收时检查来源链"));
        var contract = new AgentTodoExecutionContract(
            "补齐不可变执行合同",
            "只修改 Windows Todo 路径",
            ["持久化合同", "返回来源引用"],
            ["状态更新后合同保持不变"],
            ["源码文件不超过 800 行"]);
        var plan = new AgentTodoExecutionPlan(true,
            [AgentTodoBuiltinCapability.ProjectWrite,
             AgentTodoBuiltinCapability.RequirementSurveyWrite]);

        var created = await _store.CreateTodoAsync("alice", new AgentTodoDraft(
            room.Id, worker.Id, "执行合同", "实现并测试", AgentTodoPriority.High,
            SourceMessageId: teamSource.Message.Id,
            ExecutionContract: contract,
            ExecutionPlan: plan,
            SourceLinks:
            [
                new(direct.Id, directSource.Message.Id),
            ]));

        Assert.Equal(contract, created.Draft.ExecutionContract);
        Assert.Contains(AgentTodoBuiltinCapability.RequirementSurveyRead,
            created.Draft.ExecutionPlan!.Capabilities);
        Assert.True(created.Draft.ExecutionPlan.SelectedAtUnixMs > 0);
        Assert.Equal(2, created.Sources.Count);
        Assert.Contains(created.Sources, value => value.MessageId == teamSource.Message.Id);
        Assert.Contains(created.Sources, value => value.MessageId == directSource.Message.Id);

        var updated = await _store.UpdateTodoAsync("alice", created.Id, created.Revision,
            AgentTodoStatus.InProgress, "开始执行");
        var reloaded = await _store.GetTodoAsync("alice", created.Id);
        Assert.NotNull(reloaded);
        Assert.Equal(contract.Objective, updated.Draft.ExecutionContract!.Objective);
        Assert.Equal(contract.Scope, updated.Draft.ExecutionContract.Scope);
        Assert.Equal(contract.Outputs, updated.Draft.ExecutionContract.Outputs);
        Assert.Equal(contract.Criteria, updated.Draft.ExecutionContract.Criteria);
        Assert.Equal(contract.Limits, updated.Draft.ExecutionContract.Limits);
        Assert.Equal(updated.Draft.ExecutionContract.Objective,
            reloaded!.Draft.ExecutionContract!.Objective);
        Assert.Equal(updated.Draft.ExecutionContract.Outputs,
            reloaded.Draft.ExecutionContract.Outputs);
        Assert.Equal(created.Draft.ExecutionPlan!.RequiresExecution,
            reloaded.Draft.ExecutionPlan!.RequiresExecution);
        Assert.Equal(created.Draft.ExecutionPlan.Capabilities,
            reloaded.Draft.ExecutionPlan.Capabilities);
        Assert.Equal(created.Draft.ExecutionPlan.SelectedAtUnixMs,
            reloaded.Draft.ExecutionPlan.SelectedAtUnixMs);
        Assert.True(created.Sources.ToHashSet().SetEquals(reloaded.Sources));

        var invalidSource = await Assert.ThrowsAsync<AgentTeamException>(() =>
            _store.CreateTodoAsync("alice", new AgentTodoDraft(room.Id, worker.Id, "无效来源",
                SourceLinks: [new(room.Id, "missing-message")])));
        Assert.Equal(AgentTeamError.NotFound, invalidSource.Code);
    }

    [Fact]
    public async Task AssetsKeepRevisionHistoryAndRejectStaleWrites()
    {
        var (manager, _, room) = await CreateConfiguredTeamAsync();
        var created = await _store.UpsertAssetAsync("alice", room.Id, null, manager.Id,
            AgentTeamAssetCategory.Plan, "计划", "v1", null);
        var updated = await _store.UpsertAssetAsync("alice", room.Id, created.Id, manager.Id,
            AgentTeamAssetCategory.Deliverable, "交付", "v2", created.Revision);

        Assert.Equal(2, updated.Revision);
        var revisions = await _store.ListAssetRevisionsAsync("alice", created.Id);
        Assert.Equal([2, 1], revisions.Select(value => value.Revision));
        var conflict = await Assert.ThrowsAsync<AgentTeamException>(() =>
            _store.UpsertAssetAsync("alice", room.Id, created.Id, manager.Id,
                AgentTeamAssetCategory.Note, "stale", "stale", 1));
        Assert.Equal(AgentTeamError.Conflict, conflict.Code);

        var archived = await _store.ArchiveAssetAsync("alice", room.Id, created.Id,
            manager.Id, updated.Revision);
        Assert.Equal(AgentTeamAssetStatus.Archived, archived.Status);
        Assert.Empty(await _store.ListAssetsAsync("alice", room.Id));
        Assert.Single(await _store.ListAssetsAsync("alice", room.Id, includeArchived: true));
        Assert.Equal(3, (await _store.ListAssetRevisionsAsync("alice", created.Id)).Count);
    }

    [Fact]
    public async Task HeartbeatsAndRunsAreDurable()
    {
        var agent = await _store.CreateAgentAsync("alice", Draft("守护", heartbeat: true));
        var room = await _store.CreateRoomAsync("alice", "project-1", new("运维", "巡检"), agent.Id);
        var dueAt = (await _store.GetAgentAsync("alice", agent.Id))!.NextHeartbeatAtUnixMs!.Value;
        var heartbeats = await _store.EnqueueDueHeartbeatsAsync(dueAt);
        var delivery = Assert.Single(heartbeats);
        Assert.Equal(AgentDeliveryTrigger.Heartbeat, delivery.Trigger);
        Assert.Empty(await _store.EnqueueDueHeartbeatsAsync(dueAt));

        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var run = new AgentRunSummary("run-1", "alice", delivery.Id, agent.Id, room.Id,
            AgentRunStatus.Running, 1, null, now, now);
        await _store.SaveRunAsync(run);
        await _store.SaveRunAsync(run with
        {
            Status = AgentRunStatus.Completed,
            ModelCalls = 2,
            UpdatedAtUnixMs = now + 1,
        });
        var saved = Assert.Single(await _store.ListRunsAsync("alice", room.Id));
        Assert.Equal(AgentRunStatus.Completed, saved.Status);
        Assert.Equal(2, saved.ModelCalls);
    }

    [Fact]
    public async Task RequirementSurveyWaitsForHumanThenWakesManagerForResolution()
    {
        var (manager, _, room) = await CreateConfiguredTeamAsync();
        await CompleteInitialMaintenanceAsync();
        var trigger = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "需要确认范围"));
        var delivery = Assert.IsType<AgentDelivery>(await _store.ClaimNextDeliveryAsync("alice"));
        Assert.Equal(Assert.Single(trigger.Deliveries).Id, delivery.Id);
        var draft = new AgentRequirementSurveyDraft("范围确认", "确认首版交付范围",
        [
            new("scope", "首版包含哪些平台？", AgentRequirementQuestionKind.MultipleChoice,
            [
                new("windows", "Windows"),
                new("macos", "macOS"),
            ]),
            new("release", "是否立即发布？", AgentRequirementQuestionKind.SingleChoice,
            [
                new("yes", "是"),
                new("no", "否"),
            ]),
        ]);
        var survey = await _store.CreateRequirementSurveyAsync("alice", room.Id,
            manager.Id, delivery.Id, "scope-v1", draft);
        var idempotent = await _store.CreateRequirementSurveyAsync("alice", room.Id,
            manager.Id, delivery.Id, "scope-v1", draft);
        Assert.Equal(survey.Id, idempotent.Id);
        Assert.Equal(AgentRequirementSurveyStatus.Pending, survey.Status);
        Assert.Equal(room.ProjectId, survey.ProjectId);

        var invalid = await Assert.ThrowsAsync<AgentTeamException>(() =>
            _store.SubmitRequirementSurveyAsync("alice", room.ProjectId, survey.Id,
                new AgentRequirementSubmission([], "")));
        Assert.Equal(AgentTeamError.InvalidField, invalid.Code);
        var submitted = await _store.SubmitRequirementSurveyAsync("alice", room.ProjectId, survey.Id,
            new AgentRequirementSubmission(
            [
                new("scope", ["windows", "macos"]),
                new("release", ["no"]),
            ], "先完成回归"));
        Assert.Equal(AgentRequirementSurveyStatus.Submitted, submitted.Status);
        var wakeup = Assert.IsType<AgentDelivery>(await _store.ClaimNextDeliveryAsync("alice"));
        Assert.Equal(AgentDeliveryTrigger.RequirementSurvey, wakeup.Trigger);
        Assert.Equal(manager.Id, wakeup.TargetAgentId);

        var resolution = new AgentRequirementResolution("双平台对齐后发布", "# 方案\n完成双平台回归。",
        [
            new("implement", "实现", "完成 Windows 与 macOS 功能对齐", "开发", "安装包", "自动化通过"),
        ], "Windows 真机待验收", "测试报告");
        var resolved = await _store.ResolveRequirementSurveyAsync("alice", room.ProjectId,
            survey.Id, manager.Id, resolution);
        Assert.Equal(resolution, resolved.Resolution);
        Assert.NotNull(resolved.ResolvedAtUnixMs);
        var persisted = Assert.Single(await _store.ListRequirementSurveysAsync(
            "alice", room.ProjectId, AgentRequirementSurveyStatus.Submitted));
        Assert.Equal(resolved.Id, persisted.Id);
        Assert.Equal("双平台对齐后发布", persisted.Resolution?.Summary);
    }

    [Fact]
    public async Task RequirementSurveySkillAuthorizesAnActiveNonManagerMember()
    {
        var manager = await CreateAgentAsync("alice", "经理");
        var specialist = await _store.CreateAgentAsync("alice",
            Draft("调研") with
            {
                ProfessionKey = "researcher",
                DefaultSkillIds = ["requirement.survey.manage"],
            });
        var room = await _store.CreateRoomAsync(
            "alice", "project-1", new("产品团队", "交付产品"), manager.Id);
        await CompleteInitialMaintenanceAsync();
        var member = await _store.UpsertMemberAsync("alice", room.Id, specialist.Id,
            new("requirement_researcher", "负责需求调研"));
        _ = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "请确认范围", [specialist.Id]));
        var delivery = Assert.IsType<AgentDelivery>(
            await _store.ClaimNextDeliveryAsync("alice"));
        Assert.Equal(specialist.Id, delivery.TargetAgentId);
        var executor = new AgentTeamToolExecutor(_store, null!);
        var available = executor.AllDefinitions(specialist, room, delivery);
        Assert.Contains(available, value => value.Name == "skill_activate");
        Assert.DoesNotContain(available, value => value.Name == "todo_create");

        var result = await executor.ExecuteAsync(specialist, member, room, delivery,
            new AgentToolCall("survey", "requirement_survey_create", """
                {"request_key":"scope-v1","title":"范围","purpose":"确认交付范围","questions":[{"key":"platform","prompt":"目标平台？","kind":"multiple_choice","options":[{"key":"windows","label":"Windows"},{"key":"macos","label":"macOS"}]}]}
                """), CancellationToken.None);

        Assert.True(result.EndsCycle);
        var skill = await executor.ExecuteAsync(specialist, member, room, delivery,
            new AgentToolCall("skill", "skill_activate",
                "{\"skill_ref\":\"SKreq-create\"}"), CancellationToken.None);
        using var skillDocument = System.Text.Json.JsonDocument.Parse(skill.Content);
        Assert.Contains("requirement_survey_list",
            skillDocument.RootElement.GetProperty("instructions").GetString(),
            StringComparison.Ordinal);
        var resource = await executor.ExecuteAsync(specialist, member, room, delivery,
            new AgentToolCall("resource", "skill_read_resource",
                "{\"skill_ref\":\"SKreq-create\",\"relative_path\":\"references/example.md\",\"max_chars\":80}"),
            CancellationToken.None);
        using var resourceDocument = System.Text.Json.JsonDocument.Parse(resource.Content);
        Assert.True(resourceDocument.RootElement.GetProperty("truncated").GetBoolean());
        Assert.Equal(64, resourceDocument.RootElement.GetProperty("sha256").GetString()!.Length);
        var survey = Assert.Single(await _store.ListRequirementSurveysAsync(
            "alice", room.ProjectId, AgentRequirementSurveyStatus.Pending));
        Assert.Equal(specialist.Id, survey.CreatorAgentId);
    }

    [Fact]
    public async Task ExecutorSuggestsAssetUpdatesButOnlyManagerCanApplyThem()
    {
        var (manager, worker, room) = await CreateConfiguredTeamAsync();
        var toolExecutor = new AgentTeamToolExecutor(_store, null!);
        var member = Assert.Single(await _store.ListMembersAsync("alice", room.Id),
            value => value.AgentId == worker.Id);
        var delivery = new AgentDelivery("delivery", "alice", room.Id, "message", "root",
            worker.Id, AgentDeliveryTrigger.Todo, AgentDeliveryStatus.Running, 1, 0,
            "todo:test:revision:1", null, null, 1, null, 1);
        var denied = await Assert.ThrowsAsync<AgentTeamException>(() => toolExecutor.ExecuteAsync(
            worker, member, room, delivery, new AgentToolCall("call", "asset_create", """
                {"category":"Decision","title":"策略","markdown":"内容"}
                """), CancellationToken.None));
        Assert.Equal(AgentTeamError.PermissionDenied, denied.Code);

        var managerMember = Assert.Single(await _store.ListMembersAsync("alice", room.Id),
            value => value.AgentId == manager.Id);
        _ = await toolExecutor.ExecuteAsync(manager, managerMember, room,
            delivery with { TargetAgentId = manager.Id },
            new AgentToolCall("call-2", "asset_create", """
                {"category":"Decision","title":"策略","markdown":"内容"}
                """), CancellationToken.None);
        var created = Assert.Single(await _store.ListAssetsAsync("alice", room.Id));
        Assert.Equal("策略", created.Title);
        var assetReferences = new AgentRunReferenceVault();
        _ = await toolExecutor.ExecuteAsync(manager, managerMember, room,
            delivery with { TargetAgentId = manager.Id },
            new AgentToolCall("call-3", "asset_update", $$"""
                {"asset_ref":"{{assetReferences.AssetReference(room.Id, created.Id, created.Revision)}}","expected_revision":1,"category":"CurrentProgress","title":"当前进度","markdown":"已完成"}
                """), CancellationToken.None, assetReferences);
        var updated = Assert.Single(await _store.ListAssetsAsync("alice", room.Id));
        Assert.Equal(AgentTeamAssetCategory.CurrentProgress, updated.Category);
        Assert.Equal(2, updated.Revision);
    }

    private async Task<(AgentProfile Manager, AgentProfile Worker, AgentRoom Room)>
        CreateConfiguredTeamAsync()
    {
        var manager = await CreateAgentAsync("alice", "经理");
        var worker = await CreateAgentAsync("alice", "开发");
        var room = await _store.CreateRoomAsync(
            "alice", "project-1", new("产品团队", "交付产品"), manager.Id);
        await _store.UpsertMemberAsync("alice", room.Id, worker.Id,
            new("developer", "开发"));
        room = await _store.UpdateRoomAsync("alice", room.Id, room.Draft,
            manager.Id, manager.Id, AgentRoomStatus.Active);
        return (manager, worker, room);
    }

    private Task<AgentProfile> CreateAgentAsync(string owner, string name) =>
        _store.CreateAgentAsync(owner, Draft(name));

    private async Task CompleteInitialMaintenanceAsync()
    {
        var delivery = Assert.IsType<AgentDelivery>(
            await _store.ClaimNextDeliveryAsync("alice"));
        Assert.StartsWith("team-asset-maintenance:", delivery.DeduplicationKey,
            StringComparison.Ordinal);
        await _store.CompleteDeliveryAsync("alice", delivery.Id, null);
    }

    private static AgentProfileDraft Draft(string name, bool heartbeat = false) => new(
        name, $"{name}说明", $"你是{name}", "model-1",
        HeartbeatEnabled: heartbeat,
        HeartbeatIntervalSeconds: 60,
        HeartbeatPrompt: "检查团队状态");
}
