using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Presentation.Tasks;

public sealed partial class MessageTaskGraphViewModel
{
    private CancellationTokenSource? _realtimeCancellation;
    private bool _hasActiveRealtimeStream;
    private bool _pendingRealtimeRefresh;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasRealtimeError))]
    private string? _realtimeErrorMessage;

    public bool HasRealtimeError => !string.IsNullOrWhiteSpace(RealtimeErrorMessage);

    private void StartRealtime()
    {
        if (_realtimeClient is null || !HasActiveSession || _realtimeCancellation is not null ||
            string.IsNullOrWhiteSpace(Lookup?.ConversationId)) return;

        _realtimeCancellation = CancellationTokenSource.CreateLinkedTokenSource(_sessionCancellation!.Token);
        _ = ConsumeGraphRealtimeAsync(Lookup.ConversationId, Lookup.TurnId, _realtimeCancellation.Token);
    }

    private async Task ConsumeGraphRealtimeAsync(string conversationId, string? turnId, CancellationToken token)
    {
        Exception? failure = null;
        try
        {
            await foreach (var signal in _realtimeClient!.StreamConversationAsync(conversationId, token)
                               .WithCancellation(token).ConfigureAwait(false))
            {
                if (token.IsCancellationRequested) return;
                if (!string.Equals(signal.ConversationId, conversationId, StringComparison.Ordinal)) continue;
                await _dispatcher.InvokeAsync(() =>
                {
                    // A callback may have waited behind closing or reopening the panel.
                    if (!IsCurrentRealtime(token)) return;
                    var becameActive = !_hasActiveRealtimeStream;
                    _hasActiveRealtimeStream = true;
                    RealtimeErrorMessage = null;
                    var affectsGraph = signal.Kind == ConversationRealtimeKind.Reconcile ||
                        !string.IsNullOrWhiteSpace(turnId) && signal.TurnId == turnId &&
                        signal.Kind is ConversationRealtimeKind.Completed or ConversationRealtimeKind.Failed or
                            ConversationRealtimeKind.Cancelled or ConversationRealtimeKind.Persisted;
                    if (affectsGraph) _pendingRealtimeRefresh = true;
                    if (becameActive || affectsGraph) ScheduleCurrentPolling();
                }).ConfigureAwait(false);
            }
        }
        catch (OperationCanceledException) when (token.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            failure = exception;
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                if (!IsCurrentRealtime(token)) return;
                _hasActiveRealtimeStream = false;
                var completed = _realtimeCancellation;
                _realtimeCancellation = null;
                completed?.Dispose();
                if (failure is not null) RealtimeErrorMessage = failure.Message;
                ScheduleCurrentPolling();
            }).ConfigureAwait(false);
        }
    }

    private bool IsCurrentRealtime(CancellationToken token) =>
        HasActiveSession && !token.IsCancellationRequested && _realtimeCancellation?.Token == token;

    private void ScheduleCurrentPolling()
    {
        if (_loadCancellation is not null)
            SchedulePolling(new LoadRequest(_generation, _loadCancellation.Token));
    }

    private bool TryRefreshFromRealtime()
    {
        if (!_pendingRealtimeRefresh || !HasActiveSession || MessageId is null ||
            RequestedTaskId is null || Lookup is null) return false;

        // Multiple signals while a load/action/page is busy become one follow-up read.
        _pendingRealtimeRefresh = false;
        _ = RefreshCurrentAsync(isBackground: true);
        return true;
    }

    private void StopRealtime()
    {
        var previous = _realtimeCancellation;
        _realtimeCancellation = null;
        _hasActiveRealtimeStream = false;
        _pendingRealtimeRefresh = false;
        RealtimeErrorMessage = null;
        previous?.Cancel();
        previous?.Dispose();
    }
}
