using ChatOS.Connector.AgentTeams;
using ChatOS.Connector.Persistence;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class AgentStaffingProposalTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(), "chatos-staffing-tests", Guid.NewGuid().ToString("N"));
    private string DatabasePath => Path.Combine(_directory, "state.db");
    private SqliteAgentTeamStore _store = null!;

    public async Task InitializeAsync()
    {
        var database = new LocalStateDatabase(DatabasePath);
        await database.InitializeAsync();
        _store = new SqliteAgentTeamStore(database);
    }

    public Task DisposeAsync()
    {
        if (Directory.Exists(_directory)) Directory.Delete(_directory, recursive: true);
        return Task.CompletedTask;
    }

    [Fact]
    public async Task StaffingToolsRequireBothPermissionsOrLegacySteward()
    {
        var room = await CreateTeamAsync(await CreateAgentAsync("plain", []));
        var delivery = Delivery(room, "plain");
        var executor = new AgentTeamToolExecutor(_store, null!);
        var hireOnly = await CreateAgentAsync("hire", [AgentProfilePermissions.StaffHire]);
        var terminateOnly = await CreateAgentAsync("terminate",
            [AgentProfilePermissions.StaffTerminate]);
        var allowed = await CreateAgentAsync("staff", [AgentProfilePermissions.StaffHire,
            AgentProfilePermissions.StaffTerminate]);
        var legacy = await CreateAgentAsync("legacy",
            [AgentProfilePermissions.LegacyProjectSteward]);

        Assert.DoesNotContain(executor.AllDefinitions(hireOnly, room,
            delivery with { TargetAgentId = hireOnly.Id }), value =>
                value.Name == "agent_propose_member");
        Assert.DoesNotContain(executor.AllDefinitions(terminateOnly, room,
            delivery with { TargetAgentId = terminateOnly.Id }), value =>
                value.Name == "agent_propose_member");
        Assert.Contains(executor.AllDefinitions(allowed, room,
            delivery with { TargetAgentId = allowed.Id }), value =>
                value.Name == "agent_propose_member");
        Assert.Contains(executor.AllDefinitions(legacy, room,
            delivery with { TargetAgentId = legacy.Id }), value =>
                value.Name == "agent_propose_member_removal");
    }

    [Fact]
    public async Task CreationProposalIsIdempotentAndOnlyCreatesAfterApproval()
    {
        var manager = await CreateStaffManagerAsync("manager");
        var room = await CreateTeamAsync(manager);
        await CompleteAllPendingAsync();
        var delivery = await StartDeliveryAsync(room, manager.Id);
        var member = Assert.Single(await _store.ListMembersAsync("alice", room.Id), value =>
            value.AgentId == manager.Id);
        var executor = new AgentTeamToolExecutor(_store, null!);
        const string arguments = """
            {"name":"Windows QA","role":"qa","responsibility":"验证 Windows 客户端","role_prompt":"负责 Windows 质量验证","thinking_level":"high","profession_key":"quality","rationale":"补齐真机验收"}
            """;

        var first = await executor.ExecuteAsync(manager, member, room, delivery,
            new AgentToolCall("create-qa", "agent_propose_member", arguments),
            CancellationToken.None);
        var second = await executor.ExecuteAsync(manager, member, room, delivery,
            new AgentToolCall("create-qa", "agent_propose_member", arguments),
            CancellationToken.None);
        Assert.True(first.EndsCycle);
        Assert.Equal(first.Content, second.Content);
        Assert.Single(await _store.ListAgentsAsync("alice"));
        var pending = Assert.Single(await _store.ListStaffingProposalsAsync(
            "alice", room.Id, AgentStaffingProposalStatus.Pending));

        var conflict = await Assert.ThrowsAsync<AgentTeamException>(() => executor.ExecuteAsync(
            manager, member, room, delivery,
            new AgentToolCall("create-qa", "agent_propose_member", arguments.Replace(
                "Windows QA", "Other QA", StringComparison.Ordinal)), CancellationToken.None));
        Assert.Equal(AgentTeamError.Conflict, conflict.Code);

        var approved = await _store.ResolveStaffingProposalAsync(
            "alice", room.Id, pending.Id, approve: true);
        Assert.Equal(AgentStaffingProposalStatus.Approved, approved.Status);
        var createdId = Assert.IsType<string>(approved.CreatedAgentId);
        var created = Assert.IsType<AgentProfile>(await _store.GetAgentAsync("alice", createdId));
        Assert.Equal(manager.Draft.ModelConfigId, created.Draft.ModelConfigId);
        Assert.Equal("high", created.Draft.ThinkingLevel);
        Assert.Contains(await _store.ListMembersAsync("alice", room.Id), value =>
            value.AgentId == createdId && value.Draft.Role == "qa");
        Assert.Equal(approved, await _store.ResolveStaffingProposalAsync(
            "alice", room.Id, pending.Id, approve: true));
        var opposite = await Assert.ThrowsAsync<AgentTeamException>(() =>
            _store.ResolveStaffingProposalAsync("alice", room.Id, pending.Id, approve: false));
        Assert.Equal(AgentTeamError.Conflict, opposite.Code);
        var isolated = await Assert.ThrowsAsync<AgentTeamException>(() =>
            _store.ListStaffingProposalsAsync("bob", room.Id));
        Assert.Equal(AgentTeamError.NotFound, isolated.Code);
    }

    [Fact]
    public async Task DirectCreationKeepsAgentReusableWithoutJoiningTheDirectChat()
    {
        var manager = await CreateStaffManagerAsync("manager");
        var direct = await _store.OpenHumanAgentDirectAsync("alice", manager.Id);
        var delivery = await StartDeliveryAsync(direct, manager.Id);
        var draft = new AgentStaffingProposalDraft(AgentStaffingProposalKind.CreateAgent,
            Name: "研究员", Role: "researcher", RolePrompt: "负责研究",
            ProfessionKey: "research");
        var proposal = await _store.CreateStaffingProposalAsync("alice", direct.Id,
            manager.Id, delivery.Id, "create-researcher", draft);

        var approved = await _store.ResolveStaffingProposalAsync(
            "alice", direct.Id, proposal.Id, approve: true);
        Assert.NotNull(approved.CreatedAgentId);
        Assert.Single(await _store.ListMembersAsync("alice", direct.Id));
        Assert.Equal(2, (await _store.ListAgentsAsync("alice")).Count);
    }

    [Fact]
    public async Task ExistingMembershipAndRemovalAreAtomicAndPreserveProfiles()
    {
        var manager = await CreateStaffManagerAsync("manager");
        var staffingMember = await CreateStaffManagerAsync("staffing");
        var reusable = await CreateAgentAsync("reusable", []);
        var target = await CreateTeamAsync(manager);
        await _store.UpsertMemberAsync("alice", target.Id, staffingMember.Id,
            new("staffing", "管理成员"));
        await CompleteAllPendingAsync();
        var delivery = await StartDeliveryAsync(target, staffingMember.Id);
        var add = await _store.CreateStaffingProposalAsync("alice", target.Id,
            staffingMember.Id, delivery.Id, "add-existing",
            new(AgentStaffingProposalKind.AddExistingAgent, Role: "developer",
                Responsibility: "实现功能", TargetRoomId: target.Id,
                TargetAgentId: reusable.Id));
        Assert.DoesNotContain(await _store.ListMembersAsync("alice", target.Id), value =>
            value.AgentId == reusable.Id);
        await _store.ResolveStaffingProposalAsync("alice", target.Id, add.Id, approve: true);
        Assert.Contains(await _store.ListMembersAsync("alice", target.Id), value =>
            value.AgentId == reusable.Id);

        var remove = await _store.CreateStaffingProposalAsync("alice", target.Id,
            staffingMember.Id, delivery.Id, "remove-existing",
            new(AgentStaffingProposalKind.RemoveMember, TargetAgentId: reusable.Id,
                Reason: "职责结束", HandoffPlan: "成果已归档"));
        await _store.ResolveStaffingProposalAsync("alice", target.Id, remove.Id, approve: true);
        Assert.DoesNotContain(await _store.ListMembersAsync("alice", target.Id), value =>
            value.AgentId == reusable.Id);
        Assert.NotNull(await _store.GetAgentAsync("alice", reusable.Id));

        var removeManager = await _store.CreateStaffingProposalAsync("alice", target.Id,
            staffingMember.Id, delivery.Id, "remove-manager",
            new(AgentStaffingProposalKind.RemoveMember, TargetAgentId: manager.Id,
                Reason: "尝试移除项目经理"));
        var blocked = await Assert.ThrowsAsync<AgentTeamException>(() =>
            _store.ResolveStaffingProposalAsync("alice", target.Id, removeManager.Id,
                approve: true));
        Assert.Equal(AgentTeamError.Conflict, blocked.Code);
        Assert.Contains(await _store.ListMembersAsync("alice", target.Id), value =>
            value.AgentId == manager.Id);
    }

    [Fact]
    public async Task RejectedProposalDoesNotMutateAndSchemaV15PersistsAcrossRestart()
    {
        var manager = await CreateStaffManagerAsync("manager");
        var room = await CreateTeamAsync(manager);
        await CompleteAllPendingAsync();
        var delivery = await StartDeliveryAsync(room, manager.Id);
        var proposal = await _store.CreateStaffingProposalAsync("alice", room.Id,
            manager.Id, delivery.Id, "reject-create",
            new(AgentStaffingProposalKind.CreateAgent, Name: "不会创建",
                Role: "none", RolePrompt: "none", ProfessionKey: "general"));
        await _store.ResolveStaffingProposalAsync("alice", room.Id, proposal.Id, approve: false);
        Assert.Single(await _store.ListAgentsAsync("alice"));

        var reopenedDatabase = new LocalStateDatabase(DatabasePath);
        await reopenedDatabase.InitializeAsync();
        var reopened = new SqliteAgentTeamStore(reopenedDatabase);
        var persisted = Assert.Single(await reopened.ListStaffingProposalsAsync(
            "alice", room.Id));
        Assert.Equal(AgentStaffingProposalStatus.Rejected, persisted.Status);
        Assert.Null(persisted.CreatedAgentId);
    }

    private Task<AgentProfile> CreateStaffManagerAsync(string name) =>
        CreateAgentAsync(name, [AgentProfilePermissions.StaffHire,
            AgentProfilePermissions.StaffTerminate]);

    private Task<AgentProfile> CreateAgentAsync(string name, IReadOnlyList<string> skills) =>
        _store.CreateAgentAsync("alice", new(name, $"{name} description", $"You are {name}",
            "model-1", "medium", "general", DefaultSkillIds: skills));

    private Task<AgentRoom> CreateTeamAsync(AgentProfile manager) =>
        _store.CreateRoomAsync("alice", Guid.NewGuid().ToString("N"),
            new($"{manager.Draft.Name} team", "deliver"), manager.Id);

    private async Task<AgentDelivery> StartDeliveryAsync(AgentRoom room, string agentId)
    {
        _ = await _store.PostMessageAsync("alice", room.Id,
            new(AgentMessageSenderKind.Human, null, "please handle", [agentId]));
        AgentDelivery delivery;
        do
        {
            delivery = Assert.IsType<AgentDelivery>(await _store.ClaimNextDeliveryAsync("alice"));
            if (delivery.TargetAgentId != agentId || delivery.RoomId != room.Id)
                await _store.CompleteDeliveryAsync("alice", delivery.Id, null);
        } while (delivery.TargetAgentId != agentId || delivery.RoomId != room.Id);
        return delivery;
    }

    private async Task CompleteAllPendingAsync()
    {
        while (await _store.ClaimNextDeliveryAsync("alice") is { } delivery)
            await _store.CompleteDeliveryAsync("alice", delivery.Id, null);
    }

    private static AgentDelivery Delivery(AgentRoom room, string agentId) => new(
        "delivery", "alice", room.Id, "message", "root", agentId,
        AgentDeliveryTrigger.Mention, AgentDeliveryStatus.Running, 1, 0, "dedupe",
        null, null, 1, null, 1);
}
