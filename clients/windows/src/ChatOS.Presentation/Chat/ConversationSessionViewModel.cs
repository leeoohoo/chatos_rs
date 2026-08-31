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
    private readonly SemaphoreSlim _refreshGate = new(1, 1);
    private CancellationTokenSource? _sessionCancellation;
    private Task? _realtimeTask;
    private long _requestGeneration;

    public ConversationSessionViewModel(
        IConversationHistoryService historyService,
        IConversationCacheStore cacheStore,
        IConversationCommandService commandService,
        IConversationRuntimeSettingsService runtimeService,
        IAskUserPromptService askUserService,
        IRealtimeClient realtimeClient,
        ConversationHistoryStore historyStore,
        IUiDispatcher dispatcher,
        LocalizationViewModel? localization = null)
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
        if (_localization is not null) _localization.PropertyChanged += OnLocalizationChanged;
        Turns.CollectionChanged += (_, _) => OnPropertyChanged(nameof(IsEmpty));
        LiveProcesses.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasLiveProcesses));
        PendingPrompts.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasPendingPrompts));
        Attachments.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(HasAttachments));
            OnPropertyChanged(nameof(AttachmentTotalSizeLabel));
            OnPropertyChanged(nameof(CanSendDraft));
        };
    }

    public ObservableCollection<ConversationTurnItemViewModel> Turns { get; } = [];

    public ObservableCollection<TurnProcessItemViewModel> LiveProcesses { get; } = [];

    public ObservableCollection<AskUserPromptViewModel> PendingPrompts { get; } = [];

    public ObservableCollection<ConversationModelOption> Models { get; } = [];

    public ObservableCollection<ConversationAttachmentDraft> Attachments { get; } = [];

    public bool IsEmpty => Turns.Count == 0;

    public bool HasLiveProcesses => LiveProcesses.Count > 0;

    public bool HasPendingPrompts => PendingPrompts.Count > 0;

    public bool HasAttachments => Attachments.Count > 0;

    public bool CanSendDraft =>
        IsOpen && !IsSending && (!string.IsNullOrWhiteSpace(Draft) || HasAttachments);

    public string AttachmentTotalSizeLabel =>
        L(
            $"{Attachments.Count} 个附件 · {FormatByteCount(Attachments.Sum(static value => value.Size))}",
            $"{Attachments.Count} attachment{(Attachments.Count == 1 ? string.Empty : "s")} · {FormatByteCount(Attachments.Sum(static value => value.Size))}");

    [ObservableProperty]
    private string? _conversationId;

    [ObservableProperty]
    private string _title = "ChatOS";

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSendDraft))]
    private string _draft = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSendDraft))]
    private bool _isOpen;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private bool _isLoadingOlder;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSendDraft))]
    private bool _isSending;

    [ObservableProperty]
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
    private string? _errorMessage;

    [ObservableProperty]
    private string? _attachmentError;

    [ObservableProperty]
    private ConversationModelOption? _selectedModel;

    [ObservableProperty]
    private bool _reasoningEnabled;

    [ObservableProperty]
    private bool _planModeEnabled;

    [ObservableProperty]
    private bool _isApplyingSettings;

    public async Task OpenAsync(
        string? conversationId,
        string title,
        CancellationToken cancellationToken = default)
    {
        CancelCurrentSession();
        var linked = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _sessionCancellation = linked;
        var token = linked.Token;
        await _dispatcher.InvokeAsync(() => ResetVisualState(conversationId, title), token);
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
                ErrorMessage = exception.Message;
                IsLoading = false;
            }, CancellationToken.None).ConfigureAwait(false);
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
        }
        catch (OperationCanceledException) when (_sessionCancellation.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
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
                        token).ConfigureAwait(false);
                }
            }
            else
            {
                await SendNewTurnInternalAsync(
                    conversationId,
                    text,
                    outgoingAttachments,
                    token).ConfigureAwait(false);
            }

            await RefreshLatestInternalAsync(conversationId, token).ConfigureAwait(false);
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

                ErrorMessage = exception.Message;
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

    public Task SetReasoningAsync(bool enabled)
    {
        if (ConversationId is not { } conversationId ||
            IsApplyingSettings ||
            enabled == ReasoningEnabled)
        {
            return Task.CompletedTask;
        }

        return ApplySettingsAsync(async token =>
        {
            var settings = await _runtimeService.UpdateReasoningAsync(conversationId, enabled, token)
                .ConfigureAwait(false);
            await ApplyRuntimeSettingsAsync(settings, token).ConfigureAwait(false);
        });
    }

    public Task SetPlanModeAsync(bool enabled)
    {
        if (ConversationId is not { } conversationId ||
            IsApplyingSettings ||
            enabled == PlanModeEnabled)
        {
            return Task.CompletedTask;
        }

        return ApplySettingsAsync(async token =>
        {
            var settings = await _runtimeService.UpdatePlanModeAsync(conversationId, enabled, token)
                .ConfigureAwait(false);
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
        CancellationToken cancellationToken)
    {
        var snapshot = _historyStore.Snapshot(conversationId);
        var now = DateTimeOffset.UtcNow;
        var turnId = $"turn_{Guid.NewGuid():N}";
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
                    PlanModeEnabled),
                cancellationToken).ConfigureAwait(false);
        }
        catch
        {
            _historyStore.DiscardOptimisticTurn(conversationId, turnId);
            await ApplySnapshotAsync(conversationId, CancellationToken.None).ConfigureAwait(false);
            throw;
        }
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

            Turns.Clear();
            foreach (var turn in snapshot.Turns)
            {
                Turns.Add(new ConversationTurnItemViewModel(turn));
            }

            HasOlder = snapshot.HasOlder;
            UnreadNewerCount = snapshot.UnreadNewerCount;
            IsRunning = snapshot.Turns.Any(static turn => turn.Status == TurnStatus.Streaming);
        }, cancellationToken);
    }

    private async Task LoadRuntimeAsync(string conversationId, CancellationToken cancellationToken)
    {
        var settingsTask = _runtimeService.FetchAsync(conversationId, cancellationToken);
        var modelsTask = _runtimeService.FetchAvailableModelsAsync(cancellationToken);
        await Task.WhenAll(settingsTask, modelsTask).ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() =>
        {
            Models.Clear();
            foreach (var model in modelsTask.Result)
            {
                Models.Add(model);
            }

            ApplyRuntimeSettings(settingsTask.Result);
        }, cancellationToken).ConfigureAwait(false);
    }

    private async Task LoadPromptsAsync(string conversationId, CancellationToken cancellationToken)
    {
        var prompts = await _askUserService.FetchPromptsAsync(
            conversationId,
            cancellationToken: cancellationToken).ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() =>
        {
            PendingPrompts.Clear();
            foreach (var prompt in prompts.Where(static prompt => prompt.IsPending))
            {
                PendingPrompts.Add(new AskUserPromptViewModel(
                    prompt,
                    _askUserService,
                    () => LoadPromptsAsync(conversationId, cancellationToken),
                    _localization));
            }
        }, cancellationToken).ConfigureAwait(false);
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

                if (signal.Kind is ConversationRealtimeKind.Persisted or
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
        ReasoningEnabled = settings.ReasoningEnabled;
        PlanModeEnabled = settings.PlanModeEnabled;
        SelectedModel = Models.FirstOrDefault(model =>
            string.Equals(model.Id, settings.SelectedModelId, StringComparison.Ordinal));
    }

    private void ResetVisualState(string? conversationId, string title)
    {
        ConversationId = conversationId;
        Title = title;
        IsOpen = !string.IsNullOrWhiteSpace(conversationId);
        IsLoading = IsOpen;
        IsLoadingOlder = false;
        IsSending = false;
        IsRunning = false;
        HasOlder = false;
        UnreadNewerCount = 0;
        ErrorMessage = null;
        Draft = string.Empty;
        Turns.Clear();
        LiveProcesses.Clear();
        PendingPrompts.Clear();
        Attachments.Clear();
        Models.Clear();
        SelectedModel = null;
        ReasoningEnabled = false;
        PlanModeEnabled = false;
        AttachmentError = null;
    }

    private void RestoreAttachments(IEnumerable<ConversationAttachmentDraft> attachments)
    {
        var existingIds = Attachments.Select(static value => value.Id).ToHashSet(StringComparer.Ordinal);
        var insertIndex = 0;
        foreach (var attachment in attachments)
        {
            if (existingIds.Add(attachment.Id))
            {
                Attachments.Insert(insertIndex++, attachment);
            }
        }
    }

    private static ConversationAttachmentReference ToReference(ConversationAttachmentDraft value) => new(
        value.Id,
        value.Name,
        value.MimeType,
        value.Size,
        value.Kind);

    private static string FormatByteCount(long bytes)
    {
        if (bytes < 1024)
        {
            return $"{bytes} B";
        }

        if (bytes < 1024 * 1024)
        {
            return $"{bytes / 1024d:0.#} KB";
        }

        return $"{bytes / (1024d * 1024d):0.#} MB";
    }

    private string L(string chinese, string english) => _localization?.Text(chinese, english) ?? chinese;

    private void OnLocalizationChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        OnPropertyChanged(nameof(AttachmentTotalSizeLabel));
        OnPropertyChanged(nameof(UnreadNewerLabel));
    }

    private void CancelCurrentSession()
    {
        _sessionCancellation?.Cancel();
        _sessionCancellation?.Dispose();
        _sessionCancellation = null;
        _realtimeTask = null;
    }
}
