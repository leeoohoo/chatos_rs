using System.Collections.Specialized;
using System.Runtime.CompilerServices;
using System.Threading.Channels;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Tasks;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class ConversationSessionViewModelTests
{
    [Fact]
    public async Task OpenLoadsCacheThenAuthoritativeHistoryRuntimeAndPrompts()
    {
        var cached = Turn("cached", 1, 1, "cached reply");
        var server = Turn("server", 2, 2, "server reply");
        var services = new TestServices
        {
            CachedTurns = new[] { cached },
            HistoryPage = new HistoryPage(new[] { server }, null, false, 2, 1),
            Prompts = new[] { Prompt() },
        };
        using var viewModel = services.CreateViewModel();

        await viewModel.OpenAsync("conversation-a", "项目会话");

        Assert.True(viewModel.IsOpen);
        Assert.False(viewModel.IsLoading);
        Assert.Equal("项目会话", viewModel.Title);
        Assert.Equal(new[] { "cached", "server" }, viewModel.Turns.Select(static turn => turn.Id));
        Assert.Single(viewModel.PendingPrompts);
        Assert.Equal("model-1", viewModel.SelectedModel?.Id);
        Assert.True(viewModel.ReasoningEnabled);
        Assert.Equal("high", viewModel.EffectiveReasoningLevel);
        Assert.Contains("high", viewModel.ReasoningLevels);
        Assert.Equal(2, services.SavedTurns.Count);
    }

    [Fact]
    public async Task OpenCanonicalizesPersistedReasoningLevelToCatalogItemAfterRestart()
    {
        var catalogLevel = new string(['m', 'e', 'd', 'i', 'u', 'm']);
        var persistedLevel = new string(['m', 'e', 'd', 'i', 'u', 'm']);
        Assert.NotSame(catalogLevel, persistedLevel);
        var services = new TestServices
        {
            RuntimeSettings = new ConversationRuntimeSettings(
                "model-1",
                "Model",
                persistedLevel,
                true),
            ModelOptions =
            [
                new ConversationModelOption(
                    "model-1",
                    "Model",
                    "gpt-test",
                    catalogLevel,
                    ThinkingLevels: ["low", catalogLevel, "high"]),
            ],
        };
        using var viewModel = services.CreateViewModel();

        await viewModel.OpenAsync("conversation-a", "Chat");

        var canonicalLevel = Assert.Single(
            viewModel.ReasoningLevels,
            level => string.Equals(level, "medium", StringComparison.Ordinal));
        Assert.Same(canonicalLevel, viewModel.SelectedThinkingLevel);
        Assert.Equal("medium", viewModel.EffectiveReasoningLevel);
        Assert.True(viewModel.ReasoningEnabled);
    }

    [Fact]
    public async Task DuplicateRuntimeReadyRefreshKeepsReasoningLevelCollectionAndSelection()
    {
        var services = new TestServices
        {
            RuntimeSettings = new ConversationRuntimeSettings(
                "model-1",
                "Model",
                "medium",
                true),
            ModelOptions =
            [
                new ConversationModelOption(
                    "model-1",
                    "Model",
                    "gpt-test",
                    "medium",
                    ThinkingLevels: ["low", "medium", "high"]),
            ],
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var selected = viewModel.SelectedThinkingLevel;
        var collectionChanges = new List<NotifyCollectionChangedAction>();
        viewModel.ReasoningLevels.CollectionChanged += (_, args) =>
            collectionChanges.Add(args.Action);

        await viewModel.LocalAgentRuntimeDidBecomeReadyAsync();

        Assert.Empty(collectionChanges);
        Assert.Same(selected, viewModel.SelectedThinkingLevel);
        Assert.Equal("medium", viewModel.EffectiveReasoningLevel);
        Assert.True(viewModel.ReasoningEnabled);
    }

    [Fact]
    public async Task OpenMarksProgrammaticRuntimeSelectionAsApplyingSettings()
    {
        var services = new TestServices();
        using var viewModel = services.CreateViewModel();
        var applyingWhenReasoningLevelChanged = new List<bool>();
        viewModel.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName == nameof(viewModel.SelectedThinkingLevel))
            {
                applyingWhenReasoningLevelChanged.Add(viewModel.IsApplyingSettings);
            }
        };

        await viewModel.OpenAsync("conversation-a", "Chat");

        Assert.NotEmpty(applyingWhenReasoningLevelChanged);
        Assert.All(applyingWhenReasoningLevelChanged, Assert.True);
        Assert.False(viewModel.IsApplyingSettings);
    }

    [Fact]
    public async Task FailedSendRemovesOptimisticTurnAndRestoresDraft()
    {
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(Array.Empty<ConversationTurn>(), null, false, 0, 1),
            SendException = new InvalidOperationException("send failed"),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        viewModel.Draft = "请继续";

        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Equal("请继续", viewModel.Draft);
        Assert.Empty(viewModel.Turns);
        Assert.Equal("send failed", viewModel.ErrorMessage);
    }

    [Fact]
    public async Task RetryingTheSameFailedDraftReusesTheTurnIdentity()
    {
        var attempts = new List<ConversationSendCommand>();
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(Array.Empty<ConversationTurn>(), null, false, 0, 1),
            SendHandler = command =>
            {
                attempts.Add(command);
                return attempts.Count == 1
                    ? Task.FromException<ConversationCommandAck>(new TimeoutException("response lost"))
                    : Task.FromResult(new ConversationCommandAck(true, command.TurnId, "message-1"));
            },
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        viewModel.Draft = "创建唯一标记文件";

        await viewModel.SendCommand.ExecuteAsync(null);
        Assert.Equal("创建唯一标记文件", viewModel.Draft);
        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Equal(2, attempts.Count);
        Assert.Equal(attempts[0].TurnId, attempts[1].TurnId);
        Assert.Equal(string.Empty, viewModel.Draft);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task EditingAFailedDraftCreatesANewTurnIdentity()
    {
        var attempts = new List<ConversationSendCommand>();
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(Array.Empty<ConversationTurn>(), null, false, 0, 1),
            SendHandler = command =>
            {
                attempts.Add(command);
                return attempts.Count == 1
                    ? Task.FromException<ConversationCommandAck>(new TimeoutException("response lost"))
                    : Task.FromResult(new ConversationCommandAck(true, command.TurnId, "message-1"));
            },
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        viewModel.Draft = "original";

        await viewModel.SendCommand.ExecuteAsync(null);
        viewModel.Draft = "changed";
        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Equal(2, attempts.Count);
        Assert.NotEqual(attempts[0].TurnId, attempts[1].TurnId);
    }

    [Fact]
    public async Task ExplicitPluginComponentIsSentOnceAndThenCleared()
    {
        var capability = new ConversationCapabilityOption(
            "plugin",
            "方案创建器 · solution-authoring-mcp",
            "Create solution plans.",
            "solution-studio@chatos-marketplace",
            "solution-authoring-mcp");
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(Array.Empty<ConversationTurn>(), null, false, 0, 1),
            CapabilityOptions = [capability],
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        viewModel.SelectedCapability = capability;
        viewModel.Draft = "创建方案";

        await viewModel.SendCommand.ExecuteAsync(null);

        var selection = Assert.IsType<ConversationExplicitCapabilitySelection>(
            services.LastCommand?.ExplicitCapabilitySelection);
        Assert.Equal("solution-studio@chatos-marketplace", selection.PluginKey);
        Assert.Equal("solution-authoring-mcp", selection.ComponentKey);
        Assert.Equal("none", viewModel.SelectedCapability?.Kind);
    }

    [Fact]
    public async Task PluginInstallerSessionShowsBoundaryAndHidesOrdinaryPluginPicker()
    {
        var capability = new ConversationCapabilityOption(
            "plugin",
            "Ordinary Plugin",
            "Must not be selectable from the installer assistant.",
            "ordinary@catalog",
            "ordinary-mcp");
        var services = new TestServices
        {
            CapabilityOptions = [capability],
        };
        using var viewModel = services.CreateViewModel();

        await viewModel.OpenAsync(
            "conversation-installer",
            "插件安装助手",
            isPluginInstaller: true);

        Assert.True(viewModel.IsPluginInstaller);
        Assert.Contains("只在此对话可用", viewModel.PluginInstallerBoundaryText);
        Assert.False(viewModel.HasCapabilityOptions);
        Assert.Contains(viewModel.Capabilities, option => option.PluginKey == "ordinary@catalog");

        await viewModel.OpenAsync("conversation-main", "叽咕狸");

        Assert.False(viewModel.IsPluginInstaller);
        Assert.True(viewModel.HasCapabilityOptions);
    }

    [Fact]
    public async Task FailedNewTurnKeepsExplicitPluginComponentForRetry()
    {
        var capability = new ConversationCapabilityOption(
            "plugin",
            "任务执行器 · task-execution-mcp",
            "Execute frozen solution tasks.",
            "solution-studio@chatos-marketplace",
            "task-execution-mcp");
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(Array.Empty<ConversationTurn>(), null, false, 0, 1),
            CapabilityOptions = [capability],
            SendException = new InvalidOperationException("send failed"),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        viewModel.SelectedCapability = capability;
        viewModel.Draft = "执行方案";

        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Same(capability, viewModel.SelectedCapability);
        Assert.Equal("执行方案", viewModel.Draft);
        Assert.Equal("send failed", viewModel.ErrorMessage);
    }

    [Fact]
    public async Task ActiveTurnUsesGuidanceInsteadOfNewTurn()
    {
        var running = Turn("running", 1, 2, null, TurnStatus.Streaming);
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(new[] { running }, null, false, 2, 1),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        viewModel.Draft = "补充要求";

        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Equal(1, services.GuidanceCount);
        Assert.Equal(0, services.NewTurnCount);
        Assert.Equal("补充要求", services.LastCommand?.Content);
    }

    [Fact]
    public async Task AttachmentOnlyDraftIsSentAndAppearsInOptimisticTurn()
    {
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(Array.Empty<ConversationTurn>(), null, false, 0, 1),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var attachment = ConversationAttachmentDraft.Create(
            "design.pdf",
            "application/pdf",
            ConversationAttachmentKind.File,
            ConversationAttachmentOrigin.File,
            new byte[] { 1, 2, 3 });
        viewModel.AddAttachments(new[] { attachment });

        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Equal(1, services.NewTurnCount);
        Assert.Equal(string.Empty, services.LastCommand?.Content);
        Assert.Equal(attachment, Assert.Single(services.LastCommand!.Attachments));
        Assert.Empty(viewModel.Attachments);
    }

    [Fact]
    public async Task FailedAttachmentSendRestoresTextAndAttachment()
    {
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(Array.Empty<ConversationTurn>(), null, false, 0, 1),
            SendException = new InvalidOperationException("upload failed"),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var attachment = ConversationAttachmentDraft.Create(
            "notes.md",
            "text/markdown",
            ConversationAttachmentKind.File,
            ConversationAttachmentOrigin.File,
            new byte[] { 1 });
        viewModel.Draft = "请查看";
        viewModel.AddAttachments(new[] { attachment });

        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Equal("请查看", viewModel.Draft);
        Assert.Equal(attachment, Assert.Single(viewModel.Attachments));
        Assert.Equal("upload failed", viewModel.ErrorMessage);
    }

    [Fact]
    public async Task AttachmentValidationRejectsOversizeFileWithoutAffectingAcceptedDrafts()
    {
        var services = new TestServices();
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync(null, "Chat");
        var accepted = ConversationAttachmentDraft.Create(
            "small.txt",
            "text/plain",
            ConversationAttachmentKind.File,
            ConversationAttachmentOrigin.File,
            new byte[] { 1 });
        var rejected = ConversationAttachmentDraft.Create(
            "large.bin",
            "application/octet-stream",
            ConversationAttachmentKind.File,
            ConversationAttachmentOrigin.File,
            new byte[ConversationSessionViewModel.MaximumAttachmentBytes + 1]);

        viewModel.AddAttachments(new[] { accepted, rejected });

        Assert.Equal(accepted, Assert.Single(viewModel.Attachments));
        Assert.Contains("超过 20 MB", viewModel.AttachmentError);
    }

    [Fact]
    public async Task AttachmentValidationKeepsOnlyTheFirstTwentyDrafts()
    {
        var services = new TestServices();
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync(null, "Chat");
        var attachments = Enumerable.Range(1, ConversationSessionViewModel.MaximumAttachmentCount + 1)
            .Select(index => ConversationAttachmentDraft.Create(
                $"attachment-{index:D2}.txt",
                "text/plain",
                ConversationAttachmentKind.File,
                ConversationAttachmentOrigin.File,
                new byte[] { (byte)index }))
            .ToArray();

        viewModel.AddAttachments(attachments);

        Assert.Equal(ConversationSessionViewModel.MaximumAttachmentCount, viewModel.Attachments.Count);
        Assert.DoesNotContain(viewModel.Attachments, attachment => attachment.Name == "attachment-21.txt");
        Assert.Contains("最多添加 20 个附件", viewModel.AttachmentError);
    }

    [Fact]
    public async Task AttachmentValidationRejectsDraftThatWouldExceedTheTotalByteLimit()
    {
        var services = new TestServices();
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync(null, "Chat");
        var accepted = ConversationAttachmentDraft.Create(
            "almost-limit.bin",
            "application/octet-stream",
            ConversationAttachmentKind.File,
            ConversationAttachmentOrigin.File,
            new byte[ConversationSessionViewModel.MaximumAttachmentBytes - 1]);
        var rejected = ConversationAttachmentDraft.Create(
            "over-total.bin",
            "application/octet-stream",
            ConversationAttachmentKind.File,
            ConversationAttachmentOrigin.File,
            new byte[] { 1, 2 });

        viewModel.AddAttachments(new[] { accepted, rejected });

        Assert.Equal(accepted, Assert.Single(viewModel.Attachments));
        Assert.Contains("附件总大小不能超过 20 MB", viewModel.AttachmentError);
    }

    [Fact]
    public async Task RealtimeProcessAppearsWithoutWaitingForHistoryRefresh()
    {
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(Array.Empty<ConversationTurn>(), null, false, 0, 1),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");

        await services.Realtime.Writer.WriteAsync(new ConversationRealtimeSignal(
            "event-1",
            1,
            "conversation-a",
            "turn-1",
            ConversationRealtimeKind.Updated,
            "tool.started",
            DateTimeOffset.UtcNow,
            ProcessUpdate: new ConversationRealtimeProcessUpdate(
                "process-1",
                "正在调用工具：read_file",
                "正在读取上下文",
                "running",
                DateTimeOffset.UtcNow)));

        await WaitUntilAsync(() => viewModel.LiveProcesses.Count == 1);
        Assert.Contains("read_file", viewModel.LiveProcesses[0].Title);
    }

    [Fact]
    public async Task RealtimeReconcileRefreshesCompletedConversationTurn()
    {
        var services = new TestServices
        {
            HistoryPage = new HistoryPage(
                new[] { Turn("turn-1", 1, 1, null, TurnStatus.Streaming) },
                null,
                false,
                1,
                1),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        Assert.True(viewModel.IsRunning);

        services.HistoryPage = new HistoryPage(
            new[] { Turn("turn-1", 1, 2, "聊天功能测试通过") },
            null,
            false,
            2,
            2);
        await services.Realtime.Writer.WriteAsync(new ConversationRealtimeSignal(
            "event-2",
            2,
            "conversation-a",
            "turn-1",
            ConversationRealtimeKind.Reconcile,
            "local_conversation_changed",
            DateTimeOffset.UtcNow));

        await WaitUntilAsync(() => !viewModel.IsRunning);
        Assert.Equal(
            "聊天功能测试通过",
            Assert.Single(Assert.Single(viewModel.Turns).Replies).Text);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task PromptRefreshPreservesFieldsSelectionsAndNativeCardIdentity(bool multiple)
    {
        var services = new TestServices { Prompts = [DraftPrompt(multiple)] };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var card = Assert.Single(viewModel.PendingPrompts);
        card.Fields[0].Value = "unsent answer";
        card.Fields[1].Value = "unsent secret";
        if (multiple) card.Options[1].IsSelected = true;
        else card.SelectedSingleOption = card.Options[1];
        var collectionChanges = 0;
        viewModel.PendingPrompts.CollectionChanged += (_, _) => collectionChanges++;
        services.Prompts = [DraftPrompt(multiple) with { Title = "Updated title", Message = "Updated message", AllowsCancel = false }];

        await viewModel.RefreshLatestCommand.ExecuteAsync(null);

        Assert.Same(card, Assert.Single(viewModel.PendingPrompts));
        Assert.Equal("unsent answer", card.Fields[0].Value);
        Assert.Equal("unsent secret", card.Fields[1].Value);
        Assert.Equal("Updated title", card.Title);
        Assert.Equal("Updated message", card.Message);
        Assert.False(card.CanCancel);
        if (multiple) Assert.True(card.Options[1].IsSelected);
        else Assert.Same(card.Options[1], card.SelectedSingleOption);
        Assert.Equal(0, collectionChanges);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task RefreshDuringPromptActionKeepsTheBusyCardAndPreventsDuplicateActions(bool cancel)
    {
        var release = new TaskCompletionSource<AskUserPrompt>(TaskCreationOptions.RunContinuationsAsynchronously);
        var services = new TestServices
        {
            Prompts = [DraftPrompt()], SubmitHandler = _ => release.Task, CancelHandler = () => release.Task,
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var card = Assert.Single(viewModel.PendingPrompts);
        card.Fields[0].Value = "answer";
        var action = cancel ? card.CancelCommand.ExecuteAsync(null) : card.SubmitCommand.ExecuteAsync(null);

        await viewModel.RefreshLatestCommand.ExecuteAsync(null);

        var visible = Assert.Single(viewModel.PendingPrompts);
        Assert.Same(card, visible);
        Assert.True(visible.IsSubmitting);
        Assert.False(visible.CanSubmit);
        Assert.False(visible.CanCancel);
        release.SetResult(DraftPrompt() with { Status = cancel ? AskUserPromptStatus.Canceled : AskUserPromptStatus.Ok });
        await action;
        Assert.Empty(viewModel.PendingPrompts);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task StartingPromptActionInvalidatesEarlierRefreshSnapshots(bool cancel)
    {
        var fetchRelease = new TaskCompletionSource<IReadOnlyList<AskUserPrompt>>(TaskCreationOptions.RunContinuationsAsynchronously);
        var actionRelease = new TaskCompletionSource<AskUserPrompt>(TaskCreationOptions.RunContinuationsAsynchronously);
        var services = new TestServices
        {
            Prompts = [DraftPrompt()], SubmitHandler = _ => actionRelease.Task, CancelHandler = () => actionRelease.Task,
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var card = Assert.Single(viewModel.PendingPrompts);
        services.PromptFetchHandler = _ => fetchRelease.Task;
        var older = viewModel.RefreshLatestCommand.ExecuteAsync(null);
        await WaitUntilAsync(() => services.PromptFetchCount == 2);

        var action = cancel ? card.CancelCommand.ExecuteAsync(null) : card.SubmitCommand.ExecuteAsync(null);
        fetchRelease.SetResult([]);
        await older;
        Assert.Same(card, Assert.Single(viewModel.PendingPrompts));
        Assert.True(card.IsSubmitting);
        services.PromptFetchHandler = null;
        actionRelease.SetResult(DraftPrompt() with { Status = AskUserPromptStatus.Ok });
        await action;

        Assert.Empty(viewModel.PendingPrompts);
    }

    [Theory]
    [InlineData(AskUserPromptStatus.Ok)]
    [InlineData(AskUserPromptStatus.Canceled)]
    [InlineData(AskUserPromptStatus.Timeout)]
    [InlineData(AskUserPromptStatus.Failed)]
    public async Task ResolvedPromptRefreshClearsSecretAndDisablesTheDetachedCard(AskUserPromptStatus status)
    {
        var services = new TestServices { Prompts = [DraftPrompt()] };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var card = Assert.Single(viewModel.PendingPrompts);
        card.Fields[1].Value = "private input";
        services.Prompts = [DraftPrompt() with { Status = status }];

        await viewModel.RefreshLatestCommand.ExecuteAsync(null);

        Assert.Empty(viewModel.PendingPrompts);
        Assert.Equal(string.Empty, card.Fields[1].Value);
        Assert.False(card.CanSubmit);
        Assert.False(card.CanCancel);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OlderPromptRefreshCannotReplaceNewerResultsOrPublishItsError(bool fail)
    {
        var release = new TaskCompletionSource<IReadOnlyList<AskUserPrompt>>(TaskCreationOptions.RunContinuationsAsynchronously);
        var services = new TestServices { Prompts = [DraftPrompt()] };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var card = Assert.Single(viewModel.PendingPrompts);
        card.Fields[0].Value = "draft";
        services.PromptFetchHandler = _ => release.Task;
        var older = viewModel.RefreshLatestCommand.ExecuteAsync(null);
        await WaitUntilAsync(() => services.PromptFetchCount == 2);
        services.PromptFetchHandler = null;
        services.Prompts = [DraftPrompt() with { Title = "newest" }];
        await viewModel.RefreshLatestCommand.ExecuteAsync(null);

        if (fail) release.SetException(new IOException("obsolete fetch failed"));
        else release.SetResult([DraftPrompt() with { Title = "obsolete" }]);
        await older;

        Assert.Same(card, Assert.Single(viewModel.PendingPrompts));
        Assert.Equal("newest", card.Title);
        Assert.Equal("draft", card.Fields[0].Value);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task PromptRefreshQueuedOnUiCannotOverwriteANewerRefresh()
    {
        var dispatcher = new PausingPromptDispatcher();
        var services = new TestServices { Prompts = [DraftPrompt()] };
        using var viewModel = services.CreateViewModel(dispatcher);
        await viewModel.OpenAsync("conversation-a", "Chat");
        services.PromptFetchHandler = _ =>
        {
            dispatcher.PauseNext = true;
            return Task.FromResult<IReadOnlyList<AskUserPrompt>>([DraftPrompt() with { Title = "obsolete" }]);
        };
        var older = viewModel.RefreshLatestCommand.ExecuteAsync(null);
        await dispatcher.Entered.Task.WaitAsync(TimeSpan.FromSeconds(2));
        services.PromptFetchHandler = null;
        services.Prompts = [DraftPrompt() with { Title = "newest" }];
        await viewModel.RefreshLatestCommand.ExecuteAsync(null);
        dispatcher.Release();
        await older;

        Assert.Equal("newest", Assert.Single(viewModel.PendingPrompts).Title);
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public async Task PreviousSessionPromptFetchCannotChangeNewSession(bool sameConversation, bool fail)
    {
        var release = new TaskCompletionSource<IReadOnlyList<AskUserPrompt>>(TaskCreationOptions.RunContinuationsAsynchronously);
        var services = new TestServices { Prompts = [DraftPrompt()] };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        services.PromptFetchHandler = _ => release.Task; // Simulate a transport ignoring cancellation.
        var older = viewModel.RefreshLatestCommand.ExecuteAsync(null);
        await WaitUntilAsync(() => services.PromptFetchCount == 2);
        services.PromptFetchHandler = null;
        var nextId = sameConversation ? "conversation-a" : "conversation-b";
        services.Prompts = [DraftPrompt() with { ConversationId = nextId, Title = "new session" }];
        await viewModel.OpenAsync(nextId, "New chat");
        var currentCard = Assert.Single(viewModel.PendingPrompts);
        currentCard.Fields[0].Value = "new draft";

        if (fail) release.SetException(new IOException("old session failed"));
        else release.SetResult([DraftPrompt()]);
        await older;

        Assert.Same(currentCard, Assert.Single(viewModel.PendingPrompts));
        Assert.Equal("new session", currentCard.Title);
        Assert.Equal("new draft", currentCard.Fields[0].Value);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task RuntimeBecomingReadyReloadsModelsAfterInitialBootstrapFailure()
    {
        var services = new TestServices { RuntimeConfigured = false };
        using var viewModel = services.CreateViewModel();

        await viewModel.OpenAsync("conversation-a", "Chat");

        Assert.Contains("not configured", viewModel.ErrorMessage);
        Assert.Empty(viewModel.Models);
        viewModel.Draft = "keep this composer draft";
        services.RuntimeConfigured = true;

        await viewModel.LocalAgentRuntimeDidBecomeReadyAsync();

        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal("model-1", Assert.Single(viewModel.Models).Id);
        Assert.Equal("model-1", viewModel.SelectedModel?.Id);
        Assert.Equal("keep this composer draft", viewModel.Draft);
        Assert.False(viewModel.IsLoading);
    }

    [Fact]
    public async Task StaleRuntimeSelectionKeepsCatalogVisibleUntilHumanSelectsAReplacement()
    {
        var services = new TestServices
        {
            RuntimeFetchException = new InvalidOperationException(
                "The selected Local Agent model revision is unavailable."),
        };
        using var viewModel = services.CreateViewModel();

        await viewModel.OpenAsync("conversation-a", "Chat");

        var available = Assert.Single(viewModel.Models);
        Assert.Null(viewModel.SelectedModel);
        Assert.Empty(viewModel.ReasoningLevels);
        Assert.Contains("请选择当前可用模型", viewModel.ErrorMessage, StringComparison.Ordinal);
        Assert.Contains("不会自动改写", viewModel.ErrorMessage, StringComparison.Ordinal);

        services.RuntimeFetchException = null;
        await viewModel.SelectModelAsync(available);

        Assert.Equal("model-1", services.LastUpdatedModelId);
        Assert.Equal("model-1", viewModel.SelectedModel?.Id);
        Assert.Contains("high", viewModel.ReasoningLevels);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public async Task OldPromptActionCannotReloadNewSessionAndDropsItsSecret(bool cancel, bool fail)
    {
        var release = new TaskCompletionSource<AskUserPrompt>(TaskCreationOptions.RunContinuationsAsynchronously);
        var services = new TestServices
        {
            Prompts = [DraftPrompt()], SubmitHandler = _ => release.Task, CancelHandler = () => release.Task,
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var card = Assert.Single(viewModel.PendingPrompts);
        card.Fields[1].Value = "private input";
        var action = cancel ? card.CancelCommand.ExecuteAsync(null) : card.SubmitCommand.ExecuteAsync(null);
        services.Prompts = [DraftPrompt() with { ConversationId = "conversation-b" }];
        await viewModel.OpenAsync("conversation-b", "Next");
        var currentCard = Assert.Single(viewModel.PendingPrompts);
        var fetchCount = services.PromptFetchCount;

        if (fail) release.SetException(new IOException("old action failed"));
        else release.SetResult(DraftPrompt() with { Status = AskUserPromptStatus.Ok });
        await action;

        Assert.Equal(fetchCount, services.PromptFetchCount);
        Assert.Same(currentCard, Assert.Single(viewModel.PendingPrompts));
        Assert.Equal(string.Empty, card.Fields[1].Value);
        Assert.False(card.CanSubmit);
        Assert.False(card.CanCancel);
        Assert.Null(card.ErrorMessage);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task MissingPromptDropsItsSecretAndCannotBeSubmittedFromADetachedControl()
    {
        var services = new TestServices { Prompts = [DraftPrompt()] };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var card = Assert.Single(viewModel.PendingPrompts);
        card.Fields[1].Value = "private input";
        services.Prompts = [];
        await viewModel.RefreshLatestCommand.ExecuteAsync(null);
        var fetchCount = services.PromptFetchCount;

        await card.SubmitCommand.ExecuteAsync(null);
        await card.CancelCommand.ExecuteAsync(null);

        Assert.False(viewModel.HasPendingPrompts);
        Assert.Equal(string.Empty, card.Fields[1].Value);
        Assert.Equal(fetchCount, services.PromptFetchCount);
        Assert.False(card.CanSubmit);
        Assert.False(card.CanCancel);
    }

    [Fact]
    public async Task ChangedPromptInputsReplaceTheCardAndDiscardOldSecret()
    {
        var services = new TestServices { Prompts = [DraftPrompt()] };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var original = Assert.Single(viewModel.PendingPrompts);
        original.Fields[1].Value = "private input";
        services.Prompts = [DraftPrompt() with { Fields = [new("new", "New field", null, null, "new default", true, false, false)] }];

        await viewModel.RefreshLatestCommand.ExecuteAsync(null);

        var current = Assert.Single(viewModel.PendingPrompts);
        Assert.NotSame(original, current);
        Assert.Equal("new default", Assert.Single(current.Fields).Value);
        Assert.Equal(string.Empty, original.Fields[1].Value);
        Assert.False(original.CanSubmit);
    }

    [Fact]
    public async Task RefreshAfterFailedSubmissionPreservesDraftAndErrorForRetry()
    {
        var services = new TestServices
        {
            Prompts = [DraftPrompt()], SubmitHandler = _ => Task.FromException<AskUserPrompt>(new IOException("try again")),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var card = Assert.Single(viewModel.PendingPrompts);
        card.Fields[0].Value = "draft";
        card.Fields[1].Value = "private input";
        await card.SubmitCommand.ExecuteAsync(null);

        await viewModel.RefreshLatestCommand.ExecuteAsync(null);

        Assert.Same(card, Assert.Single(viewModel.PendingPrompts));
        Assert.Equal("draft", card.Fields[0].Value);
        Assert.Equal("private input", card.Fields[1].Value);
        Assert.Equal("try again", card.ErrorMessage);
        Assert.True(card.CanSubmit);
    }

    [Fact]
    public async Task RealtimePromptInsertionAndRemovalPreserveTheOtherDraft()
    {
        var services = new TestServices { Prompts = [DraftPrompt()] };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var card = Assert.Single(viewModel.PendingPrompts);
        card.Fields[0].Value = "draft";
        services.Prompts = [DraftPrompt() with { Id = "new-prompt" }, DraftPrompt()];
        await services.Realtime.Writer.WriteAsync(new ConversationRealtimeSignal(
            "prompt-event", 1, "conversation-a", "turn-1", ConversationRealtimeKind.Updated,
            "ask_user.updated", DateTimeOffset.UtcNow,
            AskUserPromptUpdate: new("prompt-1", "conversation-a", "turn-1", "updated", "pending")));
        await WaitUntilAsync(() => viewModel.PendingPrompts.Count == 2);
        Assert.Same(card, viewModel.PendingPrompts[1]);
        services.Prompts = [DraftPrompt()];

        await viewModel.RefreshLatestCommand.ExecuteAsync(null);

        Assert.Same(card, Assert.Single(viewModel.PendingPrompts));
        Assert.Equal("draft", card.Fields[0].Value);
    }

    [Fact]
    public async Task ResolvedPromptCannotReappearFromStaleSnapshotsButNewSessionStartsFresh()
    {
        var services = new TestServices { Prompts = [DraftPrompt()] };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        await Assert.Single(viewModel.PendingPrompts).SubmitCommand.ExecuteAsync(null);
        await viewModel.RefreshLatestCommand.ExecuteAsync(null);
        Assert.Empty(viewModel.PendingPrompts);

        await viewModel.OpenAsync("conversation-a", "Reopened");

        Assert.True(Assert.Single(viewModel.PendingPrompts).CanSubmit);
    }

    [Fact]
    public async Task CurrentFetchFailureShowsAnErrorAndKeepsTheEditableDraft()
    {
        var services = new TestServices { Prompts = [DraftPrompt()] };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var card = Assert.Single(viewModel.PendingPrompts);
        card.Fields[0].Value = "draft";
        services.PromptFetchHandler = _ => Task.FromException<IReadOnlyList<AskUserPrompt>>(new IOException("fetch failed"));

        await viewModel.RefreshLatestCommand.ExecuteAsync(null);

        Assert.Same(card, Assert.Single(viewModel.PendingPrompts));
        Assert.Equal("draft", card.Fields[0].Value);
        Assert.Equal("fetch failed", viewModel.ErrorMessage);
        Assert.True(card.CanSubmit);
    }

    [Fact]
    public async Task UnchangedRefreshKeepsReplyDetailsAndProcessExpansionWithoutResettingTimeline()
    {
        var turn = InspectorTurn();
        var services = new TestServices { HistoryPage = new([turn], null, false, 1, 1) };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var item = Assert.Single(viewModel.Turns);
        var reply = Assert.Single(item.Replies);
        item.IsProcessExpanded = true;
        await reply.ToggleInspectorAsync(TaskReplyInspectorSection.Detail,
            (_, _) => Task.FromResult(ConversationReplyInspectorTests.Snapshot("loaded")));
        var collectionChanges = 0;
        viewModel.Turns.CollectionChanged += (_, _) => collectionChanges++;
        services.HistoryPage = new([turn with { AssistantReplies = turn.AssistantReplies.ToArray() }], "older", true, 1, 2);

        await viewModel.RefreshLatestCommand.ExecuteAsync(null);

        Assert.Same(item, Assert.Single(viewModel.Turns));
        Assert.Same(reply, Assert.Single(item.Replies));
        Assert.True(item.IsProcessExpanded);
        Assert.True(reply.IsDetailExpanded);
        Assert.Equal("loaded", reply.InspectorTask?.Title);
        Assert.True(viewModel.HasOlder);
        Assert.Equal(0, collectionChanges);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task UnchangedRefreshKeepsPendingInspectorAndAcceptsItsCurrentResult(bool fail)
    {
        var services = new TestServices { HistoryPage = new([InspectorTurn()], null, false, 1, 1) };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var reply = Assert.Single(Assert.Single(viewModel.Turns).Replies);
        var pending = new TaskCompletionSource<TaskReplyInspectorSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken token = default;
        var loading = reply.ToggleInspectorAsync(TaskReplyInspectorSection.Process, (_, cancellation) =>
        {
            token = cancellation;
            return pending.Task;
        });

        await viewModel.RefreshLatestCommand.ExecuteAsync(null);

        Assert.Same(reply, Assert.Single(Assert.Single(viewModel.Turns).Replies));
        Assert.False(token.IsCancellationRequested);
        Assert.True(reply.IsInspectorLoading);
        Assert.True(reply.IsProcessExpanded);
        if (fail) pending.SetException(new IOException("current error"));
        else pending.SetResult(ConversationReplyInspectorTests.Snapshot("current result"));
        await loading;
        Assert.False(reply.IsInspectorLoading);
        if (fail) Assert.Equal("current error", reply.InspectorError);
        else Assert.Equal("current result", reply.InspectorTask?.Title);
    }

    [Fact]
    public async Task UnchangedRefreshKeepsInspectorErrorUntilTheUserRetries()
    {
        var services = new TestServices { HistoryPage = new([InspectorTurn()], null, false, 1, 1) };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var reply = Assert.Single(Assert.Single(viewModel.Turns).Replies);
        await reply.ToggleInspectorAsync(TaskReplyInspectorSection.Detail,
            (_, _) => Task.FromException<TaskReplyInspectorSnapshot>(new IOException("read failed")));

        await viewModel.RefreshLatestCommand.ExecuteAsync(null);

        Assert.Same(reply, Assert.Single(Assert.Single(viewModel.Turns).Replies));
        Assert.True(reply.IsDetailExpanded);
        Assert.Equal("read failed", reply.InspectorError);
        Assert.False(reply.IsInspectorLoading);
    }

    [Theory]
    [InlineData("append")]
    [InlineData("older")]
    [InlineData("update")]
    [InlineData("reorder")]
    public async Task ChangesToOtherTurnsKeepTheOpenReplyAndProcessWithoutReset(string change)
    {
        var inspected = InspectorTurn();
        var other = Turn("other", 3, 1, "original");
        var services = new TestServices { HistoryPage = new([inspected, other], "older", true, 1, 1) };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var item = viewModel.Turns[0];
        var reply = Assert.Single(item.Replies);
        item.IsProcessExpanded = true;
        await reply.ToggleInspectorAsync(TaskReplyInspectorSection.Detail,
            (_, _) => Task.FromResult(ConversationReplyInspectorTests.Snapshot("retained")));
        var changes = new List<NotifyCollectionChangedAction>();
        viewModel.Turns.CollectionChanged += (_, args) => changes.Add(args.Action);

        if (change == "older")
        {
            services.HistoryPage = new([Turn("older", 1, 1, "old")], null, false, 2, 2);
            await viewModel.LoadOlderCommand.ExecuteAsync(null);
            Assert.Equal(new[] { "older", "callback", "other" }, viewModel.Turns.Select(turn => turn.Id));
            Assert.False(viewModel.HasOlder);
        }
        else if (change == "append")
        {
            services.HistoryPage = new([Turn("new", 4, 1, "new")], null, false, 2, 2);
            await services.Realtime.Writer.WriteAsync(new ConversationRealtimeSignal(
                "append", 2, "conversation-a", "new", ConversationRealtimeKind.Reconcile,
                "local_conversation_changed", DateTimeOffset.UtcNow));
            await WaitUntilAsync(() => viewModel.Turns.Count == 3);
            Assert.Equal(new[] { "callback", "other", "new" }, viewModel.Turns.Select(turn => turn.Id));
        }
        else
        {
            services.HistoryPage = new([Turn("other", change == "reorder" ? 1 : 3, 2, "updated")], null, false, 2, 2);
            await viewModel.RefreshLatestCommand.ExecuteAsync(null);
            Assert.Equal(change == "reorder" ? new[] { "other", "callback" } : new[] { "callback", "other" },
                viewModel.Turns.Select(turn => turn.Id));
            Assert.Equal("updated", Assert.Single(viewModel.Turns.Single(turn => turn.Id == "other").Replies).Text);
        }

        Assert.Same(item, viewModel.Turns.Single(turn => turn.Id == "callback"));
        Assert.Same(reply, Assert.Single(item.Replies));
        Assert.True(item.IsProcessExpanded);
        Assert.True(reply.IsDetailExpanded);
        Assert.Equal("retained", reply.InspectorTask?.Title);
        Assert.NotEmpty(changes);
        Assert.DoesNotContain(NotifyCollectionChangedAction.Reset, changes);
    }

    [Fact]
    public async Task FailedOptimisticSendKeepsExistingReplyOpenDuringInsertionAndRemoval()
    {
        var services = new TestServices
        {
            HistoryPage = new([InspectorTurn()], null, false, 1, 1),
            SendException = new IOException("send failed"),
        };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var item = Assert.Single(viewModel.Turns);
        var reply = Assert.Single(item.Replies);
        await reply.ToggleInspectorAsync(TaskReplyInspectorSection.Detail,
            (_, _) => Task.FromResult(ConversationReplyInspectorTests.Snapshot("retained")));
        var changes = new List<NotifyCollectionChangedAction>();
        viewModel.Turns.CollectionChanged += (_, args) =>
        {
            Assert.Same(item, viewModel.Turns[0]);
            Assert.True(reply.IsDetailExpanded);
            changes.Add(args.Action);
        };
        viewModel.Draft = "new message";

        await viewModel.SendCommand.ExecuteAsync(null);

        Assert.Same(item, Assert.Single(viewModel.Turns));
        Assert.Equal("retained", reply.InspectorTask?.Title);
        Assert.Equal("new message", viewModel.Draft);
        Assert.Equal("send failed", viewModel.ErrorMessage);
        Assert.Equal(new[] { NotifyCollectionChangedAction.Add, NotifyCollectionChangedAction.Remove }, changes);
    }

    private static ConversationTurn InspectorTurn() => Turn("callback", 2, 1, "task reply") with
    {
        ProcessEvents = [new("process", "Process", "Step", TurnStatus.Completed)],
        AssistantReplies = [new(
            new("reply", ChatMessageRole.Assistant, "task reply", DateTimeOffset.FromUnixTimeSeconds(2), []),
            new("task", "run", "completed", "completed", "conversation-a", "callback", "user-callback"))],
    };

    [Theory]
    [InlineData("switch")]
    [InlineData("refresh")]
    [InlineData("dispose")]
    public async Task ReplacingOrLeavingTimelineCancelsOldReplyInspectors(string action)
    {
        var turn = Turn("callback", 1, 1, "task reply") with
        {
            AssistantReplies = [new(
                new("reply", ChatMessageRole.Assistant, "task reply", DateTimeOffset.UtcNow, []),
                new("task", "run", "completed", "completed", "conversation-a", "callback", "user-callback"))],
        };
        var services = new TestServices { HistoryPage = new([turn], null, false, 1, 1) };
        using var viewModel = services.CreateViewModel();
        await viewModel.OpenAsync("conversation-a", "Chat");
        var reply = Assert.Single(Assert.Single(viewModel.Turns).Replies);
        var pending = new TaskCompletionSource<ChatOS.Presentation.Tasks.TaskReplyInspectorSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken token = default;
        var loading = reply.ToggleInspectorAsync(TaskReplyInspectorSection.Detail, (_, cancellation) =>
        {
            token = cancellation;
            return pending.Task;
        });

        if (action == "switch")
        {
            services.HistoryPage = new([turn with { ConversationId = "conversation-b" }], null, false, 1, 2);
            await viewModel.OpenAsync("conversation-b", "Another conversation");
            Assert.NotSame(reply, Assert.Single(Assert.Single(viewModel.Turns).Replies));
        }
        else if (action == "refresh")
        {
            services.HistoryPage = new([turn with
            {
                Revision = 2,
                AssistantReplies = [turn.AssistantReplies[0] with
                {
                    Message = turn.AssistantReplies[0].Message with { Text = "updated reply" },
                }],
            }], null, false, 2, 2);
            await viewModel.RefreshLatestCommand.ExecuteAsync(null);
            var current = Assert.Single(Assert.Single(viewModel.Turns).Replies);
            Assert.NotSame(reply, current);
            Assert.Equal("updated reply", current.Text);
        }
        else
            viewModel.Dispose();

        Assert.True(token.IsCancellationRequested);
        pending.SetResult(ConversationReplyInspectorTests.Snapshot("stale"));
        await loading;
        ConversationReplyInspectorTests.AssertClosed(reply);
    }

    private sealed class PausingPromptDispatcher : IUiDispatcher
    {
        public bool PauseNext { get; set; }
        public TaskCompletionSource Entered { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        private readonly TaskCompletionSource _release = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public async Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
        {
            if (PauseNext)
            {
                PauseNext = false;
                Entered.TrySetResult();
                await _release.Task;
            }
            cancellationToken.ThrowIfCancellationRequested();
            action();
        }

        public void Release() => _release.TrySetResult();
    }

    private static AskUserPrompt DraftPrompt(bool multiple = false) => Prompt() with
    {
        Fields = [new("text", "Answer", null, null, "", false, false, false),
            new("secret", "Secret", null, null, "", false, false, true)],
        Choice = new(multiple, [new("a", "A", null), new("b", "B", null)], [], 0, multiple ? 2 : 1),
    };

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        while (!condition())
        {
            await Task.Delay(10, timeout.Token);
        }
    }

    private static ConversationTurn Turn(
        string id,
        long sequence,
        long revision,
        string? reply,
        TurnStatus status = TurnStatus.Completed) => new(
            id,
            "conversation-a",
            sequence,
            revision,
            new ChatMessage(
                $"user-{id}",
                ChatMessageRole.User,
                "message",
                DateTimeOffset.FromUnixTimeSeconds(sequence),
                Array.Empty<ConversationAttachmentReference>()),
            Array.Empty<TurnProcessEvent>(),
            reply is null
                ? null
                : new ChatMessage(
                    $"assistant-{id}",
                    ChatMessageRole.Assistant,
                    reply,
                    DateTimeOffset.FromUnixTimeSeconds(sequence),
                    Array.Empty<ConversationAttachmentReference>()),
            Array.Empty<ConversationAssistantReply>(),
            null,
            true,
            status,
            DateTimeOffset.FromUnixTimeSeconds(sequence),
            status == TurnStatus.Streaming ? null : DateTimeOffset.FromUnixTimeSeconds(sequence));

    private static AskUserPrompt Prompt() => new(
        "prompt-1",
        "conversation-a",
        "turn-1",
        null,
        "form",
        AskUserPromptStatus.Pending,
        "需要输入",
        "请输入环境",
        true,
        null,
        Array.Empty<AskUserField>(),
        null,
        null,
        null);

    private sealed class TestServices :
        IConversationHistoryService,
        IConversationCacheStore,
        IConversationCommandService,
        IConversationRuntimeSettingsService,
        IAskUserPromptService,
        IRealtimeClient
    {
        public IReadOnlyList<ConversationTurn> CachedTurns { get; init; } = Array.Empty<ConversationTurn>();

        public bool RuntimeConfigured { get; set; } = true;

        public Exception? RuntimeFetchException { get; set; }

        public ConversationRuntimeSettings RuntimeSettings { get; init; } =
            new("model-1", "Model", "high", true);

        public IReadOnlyList<ConversationModelOption> ModelOptions { get; init; } =
            [new ConversationModelOption("model-1", "Model", "gpt-test", "high")];

        public HistoryPage HistoryPage { get; set; } = new(
            Array.Empty<ConversationTurn>(), null, false, 0, 1);

        public IReadOnlyList<AskUserPrompt> Prompts { get; set; } = Array.Empty<AskUserPrompt>();

        public IReadOnlyList<ConversationCapabilityOption> CapabilityOptions { get; init; } =
            Array.Empty<ConversationCapabilityOption>();

        public Func<CancellationToken, Task<IReadOnlyList<AskUserPrompt>>>? PromptFetchHandler { get; set; }
        public Func<AskUserSubmission, Task<AskUserPrompt>>? SubmitHandler { get; init; }
        public Func<Task<AskUserPrompt>>? CancelHandler { get; init; }
        public int PromptFetchCount { get; private set; }

        public Exception? SendException { get; init; }

        public Func<ConversationSendCommand, Task<ConversationCommandAck>>? SendHandler { get; init; }

        public List<ConversationTurn> SavedTurns { get; } = [];

        public int NewTurnCount { get; private set; }

        public int GuidanceCount { get; private set; }

        public ConversationSendCommand? LastCommand { get; private set; }

        public string? LastUpdatedModelId { get; private set; }

        public Channel<ConversationRealtimeSignal> Realtime { get; } =
            Channel.CreateUnbounded<ConversationRealtimeSignal>();

        public ConversationSessionViewModel CreateViewModel(IUiDispatcher? dispatcher = null) => new(
            this,
            this,
            this,
            this,
            this,
            this,
            new ConversationHistoryStore(),
            dispatcher ?? new ImmediateUiDispatcher());

        public Task<HistoryPage> FetchHistoryAsync(
            ConversationHistoryQuery query,
            CancellationToken cancellationToken = default) => Task.FromResult(
            HistoryPage with { RequestGeneration = query.RequestGeneration });

        public Task<IReadOnlyList<ConversationTurn>> LoadAsync(
            string conversationId,
            CancellationToken cancellationToken = default) => Task.FromResult(CachedTurns);

        public Task SaveAsync(
            string conversationId,
            IReadOnlyList<ConversationTurn> turns,
            CancellationToken cancellationToken = default)
        {
            SavedTurns.Clear();
            SavedTurns.AddRange(turns);
            return Task.CompletedTask;
        }

        public Task DeleteAsync(string conversationId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task<ConversationCommandAck> SendNewTurnAsync(
            ConversationSendCommand command,
            CancellationToken cancellationToken = default)
        {
            NewTurnCount++;
            LastCommand = command;
            if (SendHandler is not null) return SendHandler(command);
            if (SendException is not null) return Task.FromException<ConversationCommandAck>(SendException);
            return Task.FromResult(new ConversationCommandAck(true, command.TurnId, "message-1"));
        }

        public Task<ConversationCommandAck> SendGuidanceAsync(
            ConversationSendCommand command,
            CancellationToken cancellationToken = default)
        {
            GuidanceCount++;
            LastCommand = command;
            if (SendException is not null) return Task.FromException<ConversationCommandAck>(SendException);
            return Task.FromResult(new ConversationCommandAck(true, command.TurnId, null));
        }

        public Task StopTurnAsync(
            string conversationId,
            string? turnId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<ConversationRuntimeSettings> FetchAsync(
            string conversationId,
            CancellationToken cancellationToken = default)
        {
            if (!RuntimeConfigured)
            {
                return Task.FromException<ConversationRuntimeSettings>(new InvalidOperationException(
                    "Local Agent conversation settings are not configured."));
            }
            return RuntimeFetchException is null
                ? Task.FromResult(RuntimeSettings)
                : Task.FromException<ConversationRuntimeSettings>(RuntimeFetchException);
        }

        public Task<IReadOnlyList<ConversationModelOption>> FetchAvailableModelsAsync(
            CancellationToken cancellationToken = default)
        {
            if (!RuntimeConfigured)
            {
                throw new InvalidOperationException(
                    "Local Agent conversation settings are not configured.");
            }

            return Task.FromResult<IReadOnlyList<ConversationModelOption>>(
                ModelOptions);
        }

        public Task<IReadOnlyList<ConversationCapabilityOption>> FetchAvailableCapabilitiesAsync(
            CancellationToken cancellationToken = default) => Task.FromResult(CapabilityOptions);

        public Task<ConversationRuntimeSettings> UpdateModelAsync(
            string conversationId,
            string modelId,
            CancellationToken cancellationToken = default)
        {
            LastUpdatedModelId = modelId;
            return Task.FromResult(new ConversationRuntimeSettings(modelId, "Model", "high", true));
        }

        public Task<ConversationRuntimeSettings> UpdateReasoningAsync(
            string conversationId,
            bool enabled,
            CancellationToken cancellationToken = default) => Task.FromResult(
            new ConversationRuntimeSettings("model-1", "Model", "high", enabled));

        public Task<IReadOnlyList<AskUserPrompt>> FetchPromptsAsync(
            string conversationId,
            int limit = 100,
            CancellationToken cancellationToken = default)
        {
            PromptFetchCount++;
            return PromptFetchHandler?.Invoke(cancellationToken) ?? Task.FromResult(Prompts);
        }

        public Task<AskUserPrompt> SubmitAsync(
            string promptId,
            string conversationId,
            AskUserSubmission submission,
            CancellationToken cancellationToken = default) => SubmitHandler?.Invoke(submission)
                ?? Task.FromResult(Prompt() with { Status = AskUserPromptStatus.Ok });

        public Task<AskUserPrompt> CancelAsync(
            string promptId,
            string conversationId,
            CancellationToken cancellationToken = default) => CancelHandler?.Invoke()
                ?? Task.FromResult(Prompt() with { Status = AskUserPromptStatus.Canceled });

        public async IAsyncEnumerable<ConversationRealtimeSignal> StreamConversationAsync(
            string conversationId,
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            await foreach (var signal in Realtime.Reader.ReadAllAsync(cancellationToken))
            {
                yield return signal;
            }
        }

        public async IAsyncEnumerable<PetActivityEvent> StreamPetActivitiesAsync(
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            await Task.Delay(Timeout.Infinite, cancellationToken);
            yield break;
        }
    }
}
