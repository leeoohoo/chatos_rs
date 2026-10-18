using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Chat;

public sealed partial class ConversationSessionViewModel : ObservableObject, IDisposable
{
    public const int MaximumAttachmentCount = 20;
    public const int MaximumAttachmentBytes = 20 * 1024 * 1024;

    private readonly IConversationHistoryService _historyService;
    private readonly IConversationCacheStore _cacheStore;
    private readonly IConversationCommandService _commandService;
    private readonly IConversationRuntimeSettingsService _runtimeService;
    private readonly IAskUserPromptService _askUserService;
    private readonly IRealtimeClient _realtimeClient;
    private readonly ConversationHistoryStore _historyStore;
    private readonly IUiDispatcher _dispatcher;
    private readonly LocalizationViewModel? _localization;
    private readonly IRemoteConnectionMetadataService? _remoteConnections;
    private readonly SemaphoreSlim _refreshGate = new(1, 1);
    private CancellationTokenSource? _sessionCancellation;
    private Task? _realtimeTask;
    private Task? _historyRetryTask;
    private CancellationTokenSource? _historyRetryCancellation;
    private int _historyRetryAttempt;
    private long _requestGeneration;
    private PendingNewTurnRetry? _pendingNewTurnRetry;

    public ConversationSessionViewModel(
        IConversationHistoryService historyService,
        IConversationCacheStore cacheStore,
        IConversationCommandService commandService,
        IConversationRuntimeSettingsService runtimeService,
        IAskUserPromptService askUserService,
        IRealtimeClient realtimeClient,
        ConversationHistoryStore historyStore,
        IUiDispatcher dispatcher,
        LocalizationViewModel? localization = null,
        IRemoteConnectionMetadataService? remoteConnections = null)
    {
        _historyService = historyService;
        _cacheStore = cacheStore;
        _commandService = commandService;
        _runtimeService = runtimeService;
        _askUserService = askUserService;
        _realtimeClient = realtimeClient;
        _historyStore = historyStore;
        _dispatcher = dispatcher;
        _localization = localization;
        _remoteConnections = remoteConnections;
        if (_localization is not null) _localization.PropertyChanged += OnLocalizationChanged;
        Turns.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(IsEmpty));
            OnPropertyChanged(nameof(AutomationStatus));
        };
        LiveProcesses.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasLiveProcesses));
        PendingPrompts.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasPendingPrompts));
        Attachments.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(HasAttachments));
            OnPropertyChanged(nameof(AttachmentTotalSizeLabel));
            OnPropertyChanged(nameof(CanSendDraft));
        };
        Capabilities.CollectionChanged += (_, _) =>
            OnPropertyChanged(nameof(HasCapabilityOptions));
    }

    public ObservableCollection<ConversationTurnItemViewModel> Turns { get; } = [];

    public ObservableCollection<TurnProcessItemViewModel> LiveProcesses { get; } = [];

    public ObservableCollection<AskUserPromptViewModel> PendingPrompts { get; } = [];

    public ObservableCollection<ConversationModelOption> Models { get; } = [];

    public ObservableCollection<string> ReasoningLevels { get; } = [];

    public ObservableCollection<ConversationRemoteConnectionOption> RemoteConnections { get; } = [];

    public ObservableCollection<ConversationCapabilityOption> Capabilities { get; } = [];

    public ObservableCollection<ConversationAttachmentDraft> Attachments { get; } = [];

    public bool IsEmpty => Turns.Count == 0;

    public string AutomationIdentity => string.IsNullOrWhiteSpace(ConversationId)
        ? "conversation=none"
        : $"conversation={ConversationId}";

    public string AutomationStatus
    {
        get
        {
            var state = !IsOpen
                ? "closed"
                : IsLoading
                    ? "loading"
                    : IsSending
                        ? "sending"
                        : IsRunning
                            ? "running"
                            : string.IsNullOrWhiteSpace(ErrorMessage)
                                ? "ready"
                                : "failed";
            return $"state={state}; turns={Turns.Count}";
        }
    }

    public bool HasLiveProcesses => LiveProcesses.Count > 0;

    public bool HasPendingPrompts => PendingPrompts.Count > 0;

    public bool HasReasoningLevels => ReasoningLevels.Count > 0;

    public bool HasCapabilityOptions => !IsPluginInstaller && Capabilities.Count > 1;

    public string CapabilityPickerLabel => L("能力", "Capability");

    public string PluginInstallerBoundaryText => L(
        "直接告诉我想预检或安装的本地 MCP / Plugin 文件夹。安装工具只在此对话可用，并且不会获得项目编辑或通用终端权限。",
        "Tell me which local MCP or Plugin folder to inspect or install. Installation tools are available only in this conversation and have no project-editing or general terminal access.");

    public bool HasAttachments => Attachments.Count > 0;

    public bool CanSendDraft =>
        IsOpen && !IsSending && (!string.IsNullOrWhiteSpace(Draft) || HasAttachments);

    public string AttachmentTotalSizeLabel =>
        L(
            $"{Attachments.Count} 个附件 · {FormatByteCount(Attachments.Sum(static value => value.Size))}",
            $"{Attachments.Count} attachment{(Attachments.Count == 1 ? string.Empty : "s")} · {FormatByteCount(Attachments.Sum(static value => value.Size))}");

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(AutomationIdentity))]
    private string? _conversationId;

    [ObservableProperty]
    private string _title = "ChatOS";

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasCapabilityOptions))]
    private bool _isPluginInstaller;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSendDraft))]
    private string _draft = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSendDraft))]
    [NotifyPropertyChangedFor(nameof(AutomationStatus))]
    private bool _isOpen;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(AutomationStatus))]
    private bool _isLoading;

    [ObservableProperty]
    private bool _isLoadingOlder;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSendDraft))]
    [NotifyPropertyChangedFor(nameof(AutomationStatus))]
    private bool _isSending;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(AutomationStatus))]
    private bool _isRunning;

    [ObservableProperty]
    private bool _hasOlder;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasUnreadNewer))]
    [NotifyPropertyChangedFor(nameof(UnreadNewerLabel))]
    private int _unreadNewerCount;

    public bool HasUnreadNewer => UnreadNewerCount > 0;

    public string UnreadNewerLabel => L(
        $"{UnreadNewerCount} 条新任务动态",
        $"{UnreadNewerCount} new task update{(UnreadNewerCount == 1 ? string.Empty : "s")}");

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(AutomationStatus))]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _attachmentError;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(EffectiveReasoningLevel))]
    private string? _selectedThinkingLevel;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(EffectiveReasoningLevel))]
    private ConversationModelOption? _selectedModel;

    [ObservableProperty]
    private ConversationRemoteConnectionOption? _selectedRemoteConnection;

    [ObservableProperty]
    private ConversationCapabilityOption? _selectedCapability;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(EffectiveReasoningLevel))]
    private bool _reasoningEnabled;

    [ObservableProperty]
    private bool _isApplyingSettings;

    public string DefaultReasoningLevel
    {
        get
        {
            var levels = SelectedModel?.EffectiveThinkingLevels ?? Array.Empty<string>();
            if (!string.IsNullOrWhiteSpace(SelectedModel?.ThinkingLevel) &&
                levels.Contains(SelectedModel.ThinkingLevel, StringComparer.OrdinalIgnoreCase))
            {
                return SelectedModel.ThinkingLevel.Trim().ToLowerInvariant();
            }

            return levels.FirstOrDefault(level =>
                       string.Equals(level, "auto", StringComparison.OrdinalIgnoreCase))
                   ?? levels.FirstOrDefault(level =>
                       string.Equals(level, "medium", StringComparison.OrdinalIgnoreCase))
                   ?? levels.FirstOrDefault(level =>
                       !string.Equals(level, "none", StringComparison.OrdinalIgnoreCase))
                   ?? "none";
        }
    }

    public string EffectiveReasoningLevel =>
        !ReasoningEnabled
            ? "none"
            : ReasoningLevels.FirstOrDefault(level =>
                  string.Equals(level, SelectedThinkingLevel, StringComparison.OrdinalIgnoreCase))
              ?? DefaultReasoningLevel;

    public async Task OpenAsync(
        string? conversationId,
        string title,
        CancellationToken cancellationToken = default) =>
        await OpenAsync(
            conversationId,
            title,
            isPluginInstaller: false,
            cancellationToken).ConfigureAwait(false);

    public async Task OpenAsync(
        string? conversationId,
        string title,
        bool isPluginInstaller,
        CancellationToken cancellationToken = default)
    {
        CancelCurrentSession();
        var linked = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _sessionCancellation = linked;
        var token = linked.Token;
        await _dispatcher.InvokeAsync(
            () => ResetVisualState(conversationId, title, isPluginInstaller), token);
        if (string.IsNullOrWhiteSpace(conversationId))
        {
            return;
        }

        try
        {
            var cached = await _cacheStore.LoadAsync(conversationId, token).ConfigureAwait(false);
            _historyStore.MergeCachedTurns(cached, conversationId);
            await ApplySnapshotAsync(conversationId, token).ConfigureAwait(false);

            await Task.WhenAll(
                RefreshLatestInternalAsync(conversationId, token),
                LoadRuntimeAsync(conversationId, token),
                LoadPromptsAsync(conversationId, token)).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => IsLoading = false, token).ConfigureAwait(false);
            _realtimeTask = ConsumeRealtimeAsync(conversationId, token);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                ErrorMessage = PresentConversationError(exception);
                IsLoading = false;
            }, CancellationToken.None).ConfigureAwait(false);
            ScheduleHistoryRetry(conversationId);
        }
    }

    [RelayCommand]
    private async Task RefreshLatestAsync()
    {
        if (ConversationId is not { } conversationId || _sessionCancellation is null)
        {
            return;
        }

        try
        {
            await RefreshLatestInternalAsync(conversationId, _sessionCancellation.Token);
            await LoadPromptsAsync(conversationId, _sessionCancellation.Token);
            ResetHistoryRetry();
        }
        catch (OperationCanceledException) when (_sessionCancellation.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
            ScheduleHistoryRetry(conversationId);
        }
    }

    [RelayCommand]
    private async Task LoadOlderAsync()
    {
        if (ConversationId is not { } conversationId ||
            _sessionCancellation is null ||
            IsLoadingOlder)
        {
            return;
        }

        var snapshot = _historyStore.Snapshot(conversationId);
        if (!snapshot.HasOlder || string.IsNullOrWhiteSpace(snapshot.OlderCursor))
        {
            return;
        }

        IsLoadingOlder = true;
        ErrorMessage = null;
        try
        {
            var generation = Interlocked.Increment(ref _requestGeneration);
            var page = await _historyService.FetchHistoryAsync(
                new ConversationHistoryQuery(
                    conversationId,
                    10,
                    snapshot.OlderCursor,
                    generation),
                _sessionCancellation.Token).ConfigureAwait(false);
            _historyStore.MergePage(
                page,
                conversationId,
                ConversationHistoryPageOrigin.Older);
            await PersistAndApplySnapshotAsync(conversationId, _sessionCancellation.Token)
                .ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (_sessionCancellation.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsLoadingOlder = false);
        }
    }

    [RelayCommand]
    private async Task SendAsync()
    {
        if (ConversationId is not { } conversationId ||
            _sessionCancellation is null ||
            IsSending)
        {
            return;
        }

        var text = Draft.Trim();
        var outgoingAttachments = Attachments.ToArray();
        if (text.Length == 0 && outgoingAttachments.Length == 0)
        {
            return;
        }

        var token = _sessionCancellation.Token;
        var explicitSelection = ToExplicitSelection(SelectedCapability);
        var consumedExplicitSelection = false;
        IsSending = true;
        ErrorMessage = null;
        AttachmentError = null;
        Draft = string.Empty;
        Attachments.Clear();
        try
        {
            var activeTurn = _historyStore.Snapshot(conversationId).Turns
                .LastOrDefault(static turn => turn.Status == TurnStatus.Streaming && turn.Revision > 0);
            if (activeTurn is not null)
            {
                try
                {
                    await _commandService.SendGuidanceAsync(
                        new ConversationSendCommand(
                            conversationId,
                            activeTurn.Id,
                            text,
                            outgoingAttachments),
                        token).ConfigureAwait(false);
                }
                catch (GuidanceTargetInactiveException)
                {
                    await SendNewTurnInternalAsync(
                        conversationId,
                        text,
                        outgoingAttachments,
                        explicitSelection,
                        token).ConfigureAwait(false);
                    consumedExplicitSelection = explicitSelection is not null;
                }
            }
            else
            {
                await SendNewTurnInternalAsync(
                    conversationId,
                    text,
                    outgoingAttachments,
                    explicitSelection,
                    token).ConfigureAwait(false);
                consumedExplicitSelection = explicitSelection is not null;
            }

            await RefreshLatestInternalAsync(conversationId, token).ConfigureAwait(false);
            if (consumedExplicitSelection)
            {
                await _dispatcher.InvokeAsync(() =>
                    SelectedCapability = Capabilities.FirstOrDefault(), token)
                    .ConfigureAwait(false);
            }
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (Draft.Length == 0)
                {
                    Draft = text;
                }

                RestoreAttachments(outgoingAttachments);

                ErrorMessage = PresentConversationError(exception);
            }).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsSending = false).ConfigureAwait(false);
        }
    }

    public void AddAttachments(IEnumerable<ConversationAttachmentDraft> incoming)
    {
        var accepted = new List<ConversationAttachmentDraft>();
        var errors = new List<string>();
        var existingIds = Attachments.Select(static value => value.Id).ToHashSet(StringComparer.Ordinal);
        var totalBytes = Attachments.Sum(static value => value.Size);

        foreach (var attachment in incoming)
        {
            if (existingIds.Contains(attachment.Id))
            {
                continue;
            }

            if (Attachments.Count + accepted.Count >= MaximumAttachmentCount)
            {
                errors.Add(L(
                    $"单次最多添加 {MaximumAttachmentCount} 个附件",
                    $"You can add at most {MaximumAttachmentCount} attachments at a time"));
                break;
            }

            if (attachment.Size > MaximumAttachmentBytes)
            {
                errors.Add(L(
                    $"“{attachment.Name}”超过 20 MB",
                    $"“{attachment.Name}” exceeds 20 MB"));
                continue;
            }

            if (totalBytes + attachment.Size > MaximumAttachmentBytes)
            {
                errors.Add(L(
                    "附件总大小不能超过 20 MB",
                    "The total attachment size cannot exceed 20 MB"));
                continue;
            }

            accepted.Add(attachment);
            existingIds.Add(attachment.Id);
            totalBytes += attachment.Size;
        }

        foreach (var attachment in accepted)
        {
            Attachments.Add(attachment);
        }

        AttachmentError = errors.Count == 0 ? null : string.Join("；", errors);
    }

    [RelayCommand]
    private void RemoveAttachment(ConversationAttachmentDraft? attachment)
    {
        if (attachment is null)
        {
            return;
        }

        Attachments.Remove(attachment);
        if (Attachments.Count == 0)
        {
            AttachmentError = null;
        }
    }

    [RelayCommand]
    private async Task StopAsync()
    {
        if (ConversationId is not { } conversationId || _sessionCancellation is null)
        {
            return;
        }

        var activeTurn = _historyStore.Snapshot(conversationId).Turns
            .LastOrDefault(static turn => turn.Status == TurnStatus.Streaming);
        if (activeTurn is null)
        {
            return;
        }

        try
        {
            await _commandService.StopTurnAsync(
                conversationId,
                activeTurn.Id,
                _sessionCancellation.Token).ConfigureAwait(false);
            await RefreshLatestInternalAsync(conversationId, _sessionCancellation.Token)
                .ConfigureAwait(false);
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private void MarkNewerContentRead()
    {
        if (ConversationId is not { } conversationId)
        {
            return;
        }

        _historyStore.MarkNewerContentRead(conversationId);
        UnreadNewerCount = 0;
    }

    public async Task SelectModelAsync(ConversationModelOption? model)
    {
        if (ConversationId is not { } conversationId ||
            model is null ||
            IsApplyingSettings ||
            string.Equals(model.Id, SelectedModel?.Id, StringComparison.Ordinal))
        {
            return;
        }

        await ApplySettingsAsync(async token =>
        {
            var settings = await _runtimeService.UpdateModelAsync(conversationId, model.Id, token)
                .ConfigureAwait(false);
            await ApplyRuntimeSettingsAsync(settings, token).ConfigureAwait(false);
        }).ConfigureAwait(false);
    }

    public Task SetReasoningAsync(bool enabled) =>
        SetReasoningLevelAsync(enabled ? DefaultReasoningLevel : "none");

    public Task SetReasoningLevelAsync(string level)
    {
        if (ConversationId is not { } conversationId ||
            IsApplyingSettings ||
            string.IsNullOrWhiteSpace(level))
        {
            return Task.CompletedTask;
        }

        var normalized = level.Trim().ToLowerInvariant();
        if (ReasoningLevels.Count > 0 &&
            !ReasoningLevels.Contains(normalized, StringComparer.OrdinalIgnoreCase))
        {
            return Task.CompletedTask;
        }
        if (string.Equals(SelectedThinkingLevel, normalized, StringComparison.OrdinalIgnoreCase) &&
            ReasoningEnabled == !string.Equals(normalized, "none", StringComparison.Ordinal))
        {
            return Task.CompletedTask;
        }

        return ApplySettingsAsync(async token =>
        {
            var settings = await _runtimeService.UpdateReasoningLevelAsync(conversationId, normalized, token)
                .ConfigureAwait(false);
            await ApplyRuntimeSettingsAsync(settings, token).ConfigureAwait(false);
        });
    }

    public Task SetRemoteConnectionAsync(ConversationRemoteConnectionOption? connection)
    {
        if (ConversationId is not { } conversationId ||
            connection is null ||
            IsApplyingSettings ||
            string.Equals(connection.Id, SelectedRemoteConnection?.Id, StringComparison.Ordinal))
        {
            return Task.CompletedTask;
        }

        return ApplySettingsAsync(async token =>
        {
            var settings = await _runtimeService.UpdateRemoteConnectionAsync(
                conversationId,
                connection.Id,
                token).ConfigureAwait(false);
            await ApplyRuntimeSettingsAsync(settings, token).ConfigureAwait(false);
        });
    }

    public void SetViewportPinnedToBottom(bool pinned)
    {
        if (ConversationId is not { } conversationId)
        {
            return;
        }

        var lastTurnId = _historyStore.Snapshot(conversationId).Turns.LastOrDefault()?.Id ?? string.Empty;
        _historyStore.SetViewportAnchor(
            new ViewportAnchor(lastTurnId, 0, pinned),
            conversationId);
        if (pinned)
        {
            UnreadNewerCount = 0;
        }
    }

    public async Task LocalAgentRuntimeDidBecomeReadyAsync(
        CancellationToken cancellationToken = default)
    {
        if (ConversationId is not { } conversationId || _sessionCancellation is null)
        {
            return;
        }

        using var linkedCancellation = CancellationTokenSource.CreateLinkedTokenSource(
            cancellationToken,
            _sessionCancellation.Token);
        var token = linkedCancellation.Token;
        try
        {
            await LoadRuntimeAsync(conversationId, token).ConfigureAwait(false);
            if (!string.Equals(ConversationId, conversationId, StringComparison.Ordinal))
            {
                return;
            }

            await RefreshLatestInternalAsync(conversationId, token).ConfigureAwait(false);
            await LoadPromptsAsync(conversationId, token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                ErrorMessage = null;
                IsLoading = false;
            }, token).ConfigureAwait(false);
            ResetHistoryRetry();
            if (_realtimeTask is null || _realtimeTask.IsCompleted)
            {
                _realtimeTask = ConsumeRealtimeAsync(conversationId, _sessionCancellation.Token);
            }
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                ErrorMessage = PresentConversationError(exception);
                IsLoading = false;
            }, CancellationToken.None).ConfigureAwait(false);
        }
    }

    public void Dispose()
    {
        CancelCurrentSession();
        _refreshGate.Dispose();
        if (_localization is not null) _localization.PropertyChanged -= OnLocalizationChanged;
    }

    private async Task SendNewTurnInternalAsync(
        string conversationId,
        string text,
        IReadOnlyList<ConversationAttachmentDraft> attachments,
        ConversationExplicitCapabilitySelection? explicitSelection,
        CancellationToken cancellationToken)
    {
        var snapshot = _historyStore.Snapshot(conversationId);
        var now = DateTimeOffset.UtcNow;
        var retry = _pendingNewTurnRetry;
        var turnId = retry is not null && retry.Matches(
            conversationId,
            text,
            attachments,
            ReasoningEnabled,
            explicitSelection)
            ? retry.TurnId
            : $"turn_{Guid.NewGuid():N}";
        if (!string.Equals(retry?.TurnId, turnId, StringComparison.Ordinal))
        {
            _pendingNewTurnRetry = null;
        }
        var optimistic = new ConversationTurn(
            turnId,
            conversationId,
            snapshot.Turns.Select(static turn => turn.Sequence).DefaultIfEmpty().Max() + 1,
            0,
            new ChatMessage(
                $"optimistic_{Guid.NewGuid():N}",
                ChatMessageRole.User,
                text,
                now,
                attachments.Select(ToReference).ToArray()),
            Array.Empty<TurnProcessEvent>(),
            null,
            Array.Empty<ConversationAssistantReply>(),
            null,
            true,
            TurnStatus.Streaming,
            now,
            null);
        _historyStore.ApplyRealtime(
            new RealtimeTurnEvent($"optimistic-{turnId}", optimistic.Sequence, optimistic),
            false);
        await ApplySnapshotAsync(conversationId, cancellationToken).ConfigureAwait(false);
        try
        {
            await _commandService.SendNewTurnAsync(
                new ConversationSendCommand(
                    conversationId,
                    turnId,
                    text,
                    attachments,
                    ReasoningEnabled,
                    explicitSelection),
                cancellationToken).ConfigureAwait(false);
            if (string.Equals(_pendingNewTurnRetry?.TurnId, turnId, StringComparison.Ordinal))
            {
                _pendingNewTurnRetry = null;
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            _historyStore.DiscardOptimisticTurn(conversationId, turnId);
            await ApplySnapshotAsync(conversationId, CancellationToken.None).ConfigureAwait(false);
            throw;
        }
        catch
        {
            _pendingNewTurnRetry = new(
                conversationId,
                turnId,
                text,
                attachments.Select(static attachment => attachment.Id).ToArray(),
                ReasoningEnabled,
                explicitSelection);
            _historyStore.DiscardOptimisticTurn(conversationId, turnId);
            await ApplySnapshotAsync(conversationId, CancellationToken.None).ConfigureAwait(false);
            throw;
        }
    }

    private sealed record PendingNewTurnRetry(
        string ConversationId,
        string TurnId,
        string Content,
        IReadOnlyList<string> AttachmentIds,
        bool ReasoningEnabled,
        ConversationExplicitCapabilitySelection? ExplicitSelection)
    {
        internal bool Matches(
            string conversationId,
            string content,
            IReadOnlyList<ConversationAttachmentDraft> attachments,
            bool reasoningEnabled,
            ConversationExplicitCapabilitySelection? explicitSelection) =>
            string.Equals(ConversationId, conversationId, StringComparison.Ordinal) &&
            string.Equals(Content, content, StringComparison.Ordinal) &&
            ReasoningEnabled == reasoningEnabled &&
            Equals(ExplicitSelection, explicitSelection) &&
            AttachmentIds.SequenceEqual(
                attachments.Select(static attachment => attachment.Id),
                StringComparer.Ordinal);
    }

    private async Task RefreshLatestInternalAsync(
        string conversationId,
        CancellationToken cancellationToken)
    {
        await _refreshGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var generation = Interlocked.Increment(ref _requestGeneration);
            var page = await _historyService.FetchHistoryAsync(
                new ConversationHistoryQuery(conversationId, 10, null, generation),
                cancellationToken).ConfigureAwait(false);
            if (!string.Equals(ConversationId, conversationId, StringComparison.Ordinal))
            {
                return;
            }

            _historyStore.MergePage(page, conversationId);
            await PersistAndApplySnapshotAsync(conversationId, cancellationToken)
                .ConfigureAwait(false);
        }
        finally
        {
            _refreshGate.Release();
        }
    }

    private async Task PersistAndApplySnapshotAsync(
        string conversationId,
        CancellationToken cancellationToken)
    {
        var snapshot = _historyStore.Snapshot(conversationId);
        await _cacheStore.SaveAsync(conversationId, snapshot.Turns, cancellationToken)
            .ConfigureAwait(false);
        await ApplySnapshotAsync(conversationId, cancellationToken).ConfigureAwait(false);
    }

    private Task ApplySnapshotAsync(string conversationId, CancellationToken cancellationToken)
    {
        var snapshot = _historyStore.Snapshot(conversationId);
        return _dispatcher.InvokeAsync(() =>
        {
            if (!string.Equals(ConversationId, conversationId, StringComparison.Ordinal))
            {
                return;
            }

            ApplyTimelineSnapshot(snapshot.Turns);

            HasOlder = snapshot.HasOlder;
            UnreadNewerCount = snapshot.UnreadNewerCount;
            IsRunning = snapshot.Turns.Any(static turn => turn.Status == TurnStatus.Streaming);
        }, cancellationToken);
    }

    private async Task LoadRuntimeAsync(string conversationId, CancellationToken cancellationToken)
    {
        // Publish the recovery choices before validating the persisted selection. A stale
        // model revision must remain fail-closed, but it must not hide the exact catalog entry
        // that the human can explicitly select to recover the conversation.
        var models = await _runtimeService.FetchAvailableModelsAsync(cancellationToken)
            .ConfigureAwait(false);
        var capabilities = await _runtimeService.FetchAvailableCapabilitiesAsync(cancellationToken)
            .ConfigureAwait(false);
        var remotes = await LoadRemoteConnectionsAsync(cancellationToken).ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() =>
        {
            Models.Clear();
            foreach (var model in models)
            {
                Models.Add(model);
            }

            Capabilities.Clear();
            Capabilities.Add(new ConversationCapabilityOption(
                "none",
                L("自动选择", "Automatic"),
                L("由 ChatOS 根据本条消息选择必要能力", "Let ChatOS choose capabilities for this message")));
            foreach (var capability in capabilities)
            {
                Capabilities.Add(capability);
            }
            SelectedCapability = Capabilities.FirstOrDefault();

            RemoteConnections.Clear();
            RemoteConnections.Add(new ConversationRemoteConnectionOption(
                null,
                L("未绑定远端", "No remote")));
            foreach (var remote in remotes)
            {
                RemoteConnections.Add(new ConversationRemoteConnectionOption(
                    remote.Id,
                    L($"远端：{remote.Name}", $"Remote: {remote.Name}")));
            }
        }, cancellationToken).ConfigureAwait(false);

        var settings = await _runtimeService.FetchAsync(conversationId, cancellationToken)
            .ConfigureAwait(false);
        await ApplyRuntimeSettingsAsync(settings, cancellationToken).ConfigureAwait(false);
    }

    private async Task ConsumeRealtimeAsync(string conversationId, CancellationToken cancellationToken)
    {
        try
        {
            await foreach (var signal in _realtimeClient.StreamConversationAsync(
                               conversationId,
                               cancellationToken).ConfigureAwait(false))
            {
                if (signal.ProcessUpdate is { } process)
                {
                    await _dispatcher.InvokeAsync(() => UpsertProcess(process), cancellationToken)
                        .ConfigureAwait(false);
                }

                if (signal.AskUserPromptUpdate is not null)
                {
                    await LoadPromptsAsync(conversationId, cancellationToken).ConfigureAwait(false);
                }

                if (signal.Kind is ConversationRealtimeKind.Reconcile or
                    ConversationRealtimeKind.Persisted or
                    ConversationRealtimeKind.Completed or
                    ConversationRealtimeKind.Failed or
                    ConversationRealtimeKind.Cancelled)
                {
                    await RefreshLatestInternalAsync(conversationId, cancellationToken)
                        .ConfigureAwait(false);
                    await LoadPromptsAsync(conversationId, cancellationToken).ConfigureAwait(false);
                }
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
            ScheduleHistoryRetry(conversationId);
        }
    }

    private void ScheduleHistoryRetry(string conversationId)
    {
        var sessionCancellation = _sessionCancellation;
        if (sessionCancellation is null || sessionCancellation.IsCancellationRequested ||
            _historyRetryTask is not null || _historyRetryAttempt >= 4)
        {
            return;
        }

        var delays = new[] { 2, 5, 10, 20 };
        var delay = TimeSpan.FromSeconds(delays[_historyRetryAttempt++]);
        _historyRetryCancellation = CancellationTokenSource.CreateLinkedTokenSource(
            sessionCancellation.Token);
        _historyRetryTask = RetryHistoryAsync(
            conversationId,
            delay,
            _historyRetryCancellation.Token);
    }

    private async Task RetryHistoryAsync(
        string conversationId,
        TimeSpan delay,
        CancellationToken cancellationToken)
    {
        try
        {
            await Task.Delay(delay, cancellationToken).ConfigureAwait(false);
            if (!string.Equals(ConversationId, conversationId, StringComparison.Ordinal)) return;
            await RefreshLatestInternalAsync(conversationId, cancellationToken).ConfigureAwait(false);
            await LoadPromptsAsync(conversationId, cancellationToken).ConfigureAwait(false);
            ResetHistoryRetry();
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
            _historyRetryCancellation?.Dispose();
            _historyRetryCancellation = null;
            _historyRetryTask = null;
            ScheduleHistoryRetry(conversationId);
        }
    }

    private void UpsertProcess(ConversationRealtimeProcessUpdate process)
    {
        var existing = LiveProcesses.FirstOrDefault(value => value.Id == process.Id);
        if (existing is not null)
        {
            LiveProcesses.Remove(existing);
        }

        LiveProcesses.Add(new TurnProcessItemViewModel(
            process.Id,
            process.Title,
            process.Detail,
            process.Status));
        while (LiveProcesses.Count > 12)
        {
            LiveProcesses.RemoveAt(0);
        }
    }

    private async Task ApplySettingsAsync(Func<CancellationToken, Task> operation)
    {
        if (_sessionCancellation is null)
        {
            return;
        }

        IsApplyingSettings = true;
        ErrorMessage = null;
        try
        {
            await operation(_sessionCancellation.Token).ConfigureAwait(false);
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message)
                .ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() => IsApplyingSettings = false)
                .ConfigureAwait(false);
        }
    }

    private Task ApplyRuntimeSettingsAsync(
        ConversationRuntimeSettings settings,
        CancellationToken cancellationToken) =>
        _dispatcher.InvokeAsync(() => ApplyRuntimeSettings(settings), cancellationToken);

    private void ApplyRuntimeSettings(ConversationRuntimeSettings settings)
    {
        var wasApplyingSettings = IsApplyingSettings;
        IsApplyingSettings = true;
        try
        {
            SelectedModel = Models.FirstOrDefault(model =>
                string.Equals(model.Id, settings.SelectedModelId, StringComparison.Ordinal));
            var reasoningLevels = SelectedModel?.EffectiveThinkingLevels
                .Distinct(StringComparer.OrdinalIgnoreCase)
                .ToArray() ?? [];
            if (!ReasoningLevels.SequenceEqual(reasoningLevels, StringComparer.Ordinal))
            {
                ReasoningLevels.Clear();
                foreach (var level in reasoningLevels)
                {
                    ReasoningLevels.Add(level);
                }
            }
            OnPropertyChanged(nameof(HasReasoningLevels));
            OnPropertyChanged(nameof(DefaultReasoningLevel));
            // Keep an unchanged collection intact so WinUI does not discard its SelectedItem
            // during a duplicate runtime-ready refresh. Runtime settings are deserialized
            // independently from the model catalog, so bind a changed selection to the
            // canonical string instance already in ReasoningLevels.
            SelectedThinkingLevel = ReasoningLevels.FirstOrDefault(level =>
                string.Equals(level, settings.SelectedThinkingLevel, StringComparison.OrdinalIgnoreCase))
                ?? settings.SelectedThinkingLevel;
            ReasoningEnabled = settings.ReasoningEnabled;
            SelectedRemoteConnection = RemoteConnections.FirstOrDefault(connection =>
                string.Equals(connection.Id, settings.RemoteConnectionId, StringComparison.Ordinal))
                ?? RemoteConnections.FirstOrDefault();
        }
        finally
        {
            IsApplyingSettings = wasApplyingSettings;
        }
    }

    private string PresentConversationError(Exception exception)
    {
        var message = exception.Message.Trim();
        if (message.Contains(
                "selected Local Agent model revision is unavailable",
                StringComparison.OrdinalIgnoreCase))
        {
            return L(
                "此对话保存的模型版本已不可用。请选择当前可用模型以明确更新此对话；在选择前不会自动改写模型或推理配置。",
                "The saved model revision for this conversation is unavailable. Select a current model to explicitly update this conversation; its model and reasoning settings will not be rewritten before you choose.");
        }

        if (message.Contains("403 Forbidden", StringComparison.OrdinalIgnoreCase) &&
            message.Contains("vip", StringComparison.OrdinalIgnoreCase))
        {
            return L(
                "当前账号没有权限使用所选 AI 模型（vip 分组）。请在设置 → AI 模型配置中选择有权限的模型，或联系管理员开通权限。",
                "This account is not allowed to use the selected AI model (vip group). Choose an authorized model in Settings → AI model configuration, or ask an administrator to grant access.");
        }

        return message;
    }

    private static ConversationExplicitCapabilitySelection? ToExplicitSelection(
        ConversationCapabilityOption? option) =>
        option is not null && string.Equals(option.Kind, "plugin", StringComparison.Ordinal)
            ? new ConversationExplicitCapabilitySelection(
                option.Kind,
                option.DisplayName,
                option.PluginKey,
                option.ComponentKey)
            : null;

    private async Task<IReadOnlyList<RemoteConnection>> LoadRemoteConnectionsAsync(
        CancellationToken cancellationToken)
    {
        if (_remoteConnections is null)
        {
            return Array.Empty<RemoteConnection>();
        }

        try
        {
            return await _remoteConnections.ListAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (InvalidOperationException)
        {
            return Array.Empty<RemoteConnection>();
        }
    }

}
