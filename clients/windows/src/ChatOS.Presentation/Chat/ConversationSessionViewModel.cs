using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Chat;

public sealed partial class ConversationSessionViewModel : ObservableObject, IDisposable
{
    public const int MaximumAttachmentCount = 20;
    public const int MaximumAttachmentBytes = 5 * 1024 * 1024;
    public const int MaximumAttachmentTotalBytes = 6 * 1024 * 1024;

    private readonly ILocalAgentMainChatService _mainChat;
    private readonly IConversationRuntimeSettingsService _runtimeService;
    private readonly IAskUserPromptService _askUserService;
    private readonly IUiDispatcher _dispatcher;
    private readonly LocalizationViewModel? _localization;
    private readonly SemaphoreSlim _projectionRefreshGate = new(1, 1);
    private CancellationTokenSource? _sessionCancellation;
    private long _generation;
    private bool _viewportPinnedToBottom = true;

    public ConversationSessionViewModel(
        ILocalAgentMainChatService mainChat,
        IConversationRuntimeSettingsService runtimeService,
        IAskUserPromptService askUserService,
        IUiDispatcher dispatcher,
        LocalizationViewModel? localization = null)
    {
        _mainChat = mainChat;
        _runtimeService = runtimeService;
        _askUserService = askUserService;
        _dispatcher = dispatcher;
        _localization = localization;
        _mainChat.ProjectionChanged += OnProjectionChanged;
        _mainChat.ProjectionCleared += OnProjectionCleared;
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
        IsOpen && !IsSending && !IsRunning
        && (!string.IsNullOrWhiteSpace(Draft) || HasAttachments);
    public string AttachmentTotalSizeLabel => L(
        $"{Attachments.Count} 个附件 · {FormatByteCount(Attachments.Sum(static value => value.Size))}",
        $"{Attachments.Count} attachment{(Attachments.Count == 1 ? string.Empty : "s")} · {FormatByteCount(Attachments.Sum(static value => value.Size))}");
    public bool HasUnreadNewer => UnreadNewerCount > 0;
    public string UnreadNewerLabel => L(
        $"{UnreadNewerCount} 条新动态",
        $"{UnreadNewerCount} new update{(UnreadNewerCount == 1 ? string.Empty : "s")}");

    [ObservableProperty] private LocalAgentConversationScope? _scope;
    [ObservableProperty] private string? _conversationId;
    [ObservableProperty] private string _title = "ChatOS";
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSendDraft))]
    private string _draft = string.Empty;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSendDraft))]
    private bool _isOpen;
    [ObservableProperty] private bool _isLoading;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSendDraft))]
    private bool _isSending;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanSendDraft))]
    private bool _isRunning;
    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasUnreadNewer))]
    [NotifyPropertyChangedFor(nameof(UnreadNewerLabel))]
    private int _unreadNewerCount;
    [ObservableProperty] private string? _errorMessage;
    [ObservableProperty] private string? _attachmentError;
    [ObservableProperty] private ConversationModelOption? _selectedModel;
    [ObservableProperty] private bool _reasoningEnabled;
    [ObservableProperty] private bool _isApplyingSettings;

    public async Task OpenAsync(
        LocalAgentConversationScope? scope,
        string title,
        CancellationToken cancellationToken = default)
    {
        CancelCurrentSession();
        var linked = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        _sessionCancellation = linked;
        var token = linked.Token;
        var generation = Interlocked.Increment(ref _generation);
        await _dispatcher.InvokeAsync(() => ResetVisualState(scope, title), token);
        if (scope is null) return;
        try
        {
            await Task.WhenAll(
                RefreshProjectionAsync(scope, generation, token),
                LoadRuntimeAsync(scope.ThreadId, generation, token),
                LoadPromptsAsync(scope.ThreadId, generation, token)).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation) IsLoading = false;
            }, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation)
                {
                    ErrorMessage = exception.Message;
                    IsLoading = false;
                }
            }).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private async Task RefreshLatestAsync()
    {
        if (Scope is not { } scope || _sessionCancellation is null) return;
        try
        {
            await RefreshProjectionAsync(scope, _generation, _sessionCancellation.Token)
                .ConfigureAwait(false);
            await LoadPromptsAsync(scope.ThreadId, _generation, _sessionCancellation.Token)
                .ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (_sessionCancellation.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private async Task SendAsync()
    {
        if (Scope is not { } scope || _sessionCancellation is null || IsSending || IsRunning) return;
        var text = Draft.Trim();
        var outgoingAttachments = Attachments.ToArray();
        if (text.Length == 0 && outgoingAttachments.Length == 0) return;
        var token = _sessionCancellation.Token;
        var generation = _generation;
        IsSending = true;
        ErrorMessage = null;
        AttachmentError = null;
        Draft = string.Empty;
        Attachments.Clear();
        try
        {
            await _mainChat.CreateTurnAsync(
                new LocalAgentCreateConversationTurn(
                    scope,
                    $"turn-{Guid.NewGuid():N}",
                    $"message-{Guid.NewGuid():N}",
                    text.Length == 0 ? null : text,
                    outgoingAttachments),
                token).ConfigureAwait(false);
            await RefreshProjectionAsync(scope, _generation, token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation != _generation || Scope != scope) return;
                if (Draft.Length == 0) Draft = text;
                RestoreAttachments(outgoingAttachments);
                ErrorMessage = exception.Message;
            }).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation && Scope == scope) IsSending = false;
            }).ConfigureAwait(false);
        }
    }

    [RelayCommand]
    private async Task StopAsync()
    {
        if (Scope is not { } scope || _sessionCancellation is null) return;
        var active = Turns.LastOrDefault(static turn => turn.IsRunning);
        if (active is null) return;
        try
        {
            await _mainChat.CancelTurnAsync(
                scope.ThreadId,
                active.Id,
                active.RunId,
                active.RunVersion,
                _sessionCancellation.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (_sessionCancellation.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (Scope == scope) ErrorMessage = exception.Message;
            }).ConfigureAwait(false);
        }
    }

    public void AddAttachments(IEnumerable<ConversationAttachmentDraft> incoming)
    {
        var accepted = new List<ConversationAttachmentDraft>();
        var errors = new List<string>();
        var ids = Attachments.Select(static value => value.Id).ToHashSet(StringComparer.Ordinal);
        var total = Attachments.Sum(static value => value.Size);
        foreach (var attachment in incoming)
        {
            if (ids.Contains(attachment.Id)) continue;
            if (Attachments.Count + accepted.Count >= MaximumAttachmentCount)
            {
                errors.Add(L($"单次最多添加 {MaximumAttachmentCount} 个附件",
                    $"You can add at most {MaximumAttachmentCount} attachments at a time"));
                break;
            }
            if (attachment.Size > MaximumAttachmentBytes)
            {
                errors.Add(L($"“{attachment.Name}”超过 5 MB", $"“{attachment.Name}” exceeds 5 MB"));
                continue;
            }
            if (total + attachment.Size > MaximumAttachmentTotalBytes)
            {
                errors.Add(L("附件总大小不能超过 6 MB", "The total attachment size cannot exceed 6 MB"));
                continue;
            }
            accepted.Add(attachment);
            ids.Add(attachment.Id);
            total += attachment.Size;
        }
        foreach (var attachment in accepted) Attachments.Add(attachment);
        AttachmentError = errors.Count == 0 ? null : string.Join("；", errors);
    }

    [RelayCommand]
    private void RemoveAttachment(ConversationAttachmentDraft? attachment)
    {
        if (attachment is null) return;
        Attachments.Remove(attachment);
        if (Attachments.Count == 0) AttachmentError = null;
    }

    [RelayCommand]
    private void MarkNewerContentRead() => UnreadNewerCount = 0;

    public async Task SelectModelAsync(ConversationModelOption? model)
    {
        if (Scope is not { } scope || model is null || IsApplyingSettings
            || string.Equals(model.Id, SelectedModel?.Id, StringComparison.Ordinal)) return;
        var generation = _generation;
        await ApplySettingsAsync(async token =>
        {
            var settings = await _runtimeService.UpdateModelAsync(scope.ThreadId, model.Id, token)
                .ConfigureAwait(false);
            await ApplyRuntimeSettingsAsync(settings, scope, generation, token).ConfigureAwait(false);
        }).ConfigureAwait(false);
    }

    public Task SetReasoningAsync(bool enabled)
    {
        if (Scope is not { } scope || IsApplyingSettings || enabled == ReasoningEnabled)
            return Task.CompletedTask;
        var generation = _generation;
        return ApplySettingsAsync(async token =>
        {
            var settings = await _runtimeService.UpdateReasoningAsync(scope.ThreadId, enabled, token)
                .ConfigureAwait(false);
            await ApplyRuntimeSettingsAsync(settings, scope, generation, token).ConfigureAwait(false);
        });
    }

    public void SetViewportPinnedToBottom(bool pinned)
    {
        _viewportPinnedToBottom = pinned;
        if (pinned) UnreadNewerCount = 0;
    }

    public void Dispose()
    {
        CancelCurrentSession();
        _mainChat.ProjectionChanged -= OnProjectionChanged;
        _mainChat.ProjectionCleared -= OnProjectionCleared;
        _projectionRefreshGate.Dispose();
        if (_localization is not null) _localization.PropertyChanged -= OnLocalizationChanged;
    }

    private async Task RefreshProjectionAsync(
        LocalAgentConversationScope scope,
        long generation,
        CancellationToken cancellationToken)
    {
        await _projectionRefreshGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var snapshot = await _mainChat.GetConversationAsync(scope.ThreadId, cancellationToken)
                .ConfigureAwait(false);
            if (!string.Equals(snapshot.AccountId, scope.AccountId, StringComparison.Ordinal)
                || !string.Equals(snapshot.ThreadId, scope.ThreadId, StringComparison.Ordinal))
            {
                throw new InvalidDataException("The Local Agent conversation projection changed identity.");
            }
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation != _generation || Scope != scope) return;
                var oldRevision = Turns.Sum(static turn => turn.Revision);
                Turns.Clear();
                foreach (var turn in snapshot.Turns) Turns.Add(new ConversationTurnItemViewModel(turn));
                LiveProcesses.Clear();
                var active = Turns.LastOrDefault(static turn => turn.IsRunning);
                if (active is not null)
                {
                    foreach (var item in active.ProcessEvents.TakeLast(12)) LiveProcesses.Add(item);
                }
                IsRunning = active is not null;
                var newRevision = Turns.Sum(static turn => turn.Revision);
                if (!_viewportPinnedToBottom && newRevision > oldRevision) UnreadNewerCount++;
            }, cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            _projectionRefreshGate.Release();
        }
    }

    private async Task LoadRuntimeAsync(string threadId, long generation, CancellationToken cancellationToken)
    {
        var settingsTask = _runtimeService.FetchAsync(threadId, cancellationToken);
        var modelsTask = _runtimeService.FetchAvailableModelsAsync(cancellationToken);
        await Task.WhenAll(settingsTask, modelsTask).ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() =>
        {
            if (generation != _generation) return;
            Models.Clear();
            foreach (var model in modelsTask.Result) Models.Add(model);
            ApplyRuntimeSettings(settingsTask.Result);
        }, cancellationToken).ConfigureAwait(false);
    }

    private async Task LoadPromptsAsync(
        string threadId,
        long generation,
        CancellationToken cancellationToken)
    {
        var prompts = await _askUserService.FetchPromptsAsync(threadId, cancellationToken: cancellationToken)
            .ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() =>
        {
            if (generation != _generation) return;
            PendingPrompts.Clear();
            foreach (var prompt in prompts.Where(static prompt => prompt.IsPending))
            {
                PendingPrompts.Add(new AskUserPromptViewModel(
                    prompt,
                    _askUserService,
                    () => LoadPromptsAsync(threadId, generation, cancellationToken),
                    _localization));
            }
        }, cancellationToken).ConfigureAwait(false);
    }

    private async Task ApplySettingsAsync(Func<CancellationToken, Task> operation)
    {
        if (_sessionCancellation is null) return;
        var generation = _generation;
        var scope = Scope;
        IsApplyingSettings = true;
        ErrorMessage = null;
        try
        {
            await operation(_sessionCancellation.Token).ConfigureAwait(false);
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation && Scope == scope) ErrorMessage = exception.Message;
            }).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation && Scope == scope) IsApplyingSettings = false;
            }).ConfigureAwait(false);
        }
    }

    private Task ApplyRuntimeSettingsAsync(
        ConversationRuntimeSettings settings,
        LocalAgentConversationScope expectedScope,
        long generation,
        CancellationToken cancellationToken) =>
        _dispatcher.InvokeAsync(() =>
        {
            if (generation == _generation && Scope == expectedScope) ApplyRuntimeSettings(settings);
        }, cancellationToken);

    private void ApplyRuntimeSettings(ConversationRuntimeSettings settings)
    {
        ReasoningEnabled = settings.ReasoningEnabled;
        SelectedModel = Models.FirstOrDefault(model =>
            string.Equals(model.Id, settings.SelectedModelId, StringComparison.Ordinal));
    }

    private void ResetVisualState(LocalAgentConversationScope? scope, string title)
    {
        Scope = scope;
        ConversationId = scope?.ThreadId;
        Title = title;
        IsOpen = scope is not null;
        IsLoading = IsOpen;
        IsSending = false;
        IsRunning = false;
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
        AttachmentError = null;
        _viewportPinnedToBottom = true;
    }

    private void RestoreAttachments(IEnumerable<ConversationAttachmentDraft> attachments)
    {
        var ids = Attachments.Select(static value => value.Id).ToHashSet(StringComparer.Ordinal);
        var index = 0;
        foreach (var attachment in attachments)
        {
            if (ids.Add(attachment.Id)) Attachments.Insert(index++, attachment);
        }
    }

    private void OnProjectionChanged(object? sender, EventArgs e)
    {
        if (Scope is not { } scope || _sessionCancellation is null) return;
        var generation = _generation;
        var token = _sessionCancellation.Token;
        _ = RefreshProjectionSafelyAsync(scope, generation, token);
    }

    private async Task RefreshProjectionSafelyAsync(
        LocalAgentConversationScope scope,
        long generation,
        CancellationToken cancellationToken)
    {
        try
        {
            await RefreshProjectionAsync(scope, generation, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (generation == _generation) ErrorMessage = exception.Message;
            }).ConfigureAwait(false);
        }
    }

    private void OnProjectionCleared(object? sender, EventArgs e)
    {
        CancelCurrentSession();
        Interlocked.Increment(ref _generation);
        _ = _dispatcher.InvokeAsync(() => ResetVisualState(null, "ChatOS"));
    }

    private static string FormatByteCount(long bytes)
    {
        if (bytes < 1024) return $"{bytes} B";
        if (bytes < 1024 * 1024) return $"{bytes / 1024d:0.#} KB";
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
    }
}
