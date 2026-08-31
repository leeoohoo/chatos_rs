using System.Collections.Concurrent;

namespace ChatOS.Connector.Approval;

public interface IConnectorApprovalStore
{
    Task<ConnectorApprovalMode?> ReadModeAsync(CancellationToken cancellationToken = default);

    Task SaveModeAsync(ConnectorApprovalMode mode, CancellationToken cancellationToken = default);

    Task AppendAsync(
        ConnectorApprovalHistoryEntry entry,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<ConnectorApprovalHistoryEntry>> ReadHistoryAsync(
        int limit = 1_000,
        CancellationToken cancellationToken = default);
}

public sealed class CommandApprovalCoordinator
{
    private static readonly IReadOnlyList<ConnectorApprovalAction> AvailableActions =
    [
        ConnectorApprovalAction.Accept,
        ConnectorApprovalAction.AcceptForSession,
        ConnectorApprovalAction.Decline,
    ];

    private readonly object _gate = new();
    private readonly ConcurrentDictionary<string, PendingState> _pendingById = new(StringComparer.Ordinal);
    private readonly Dictionary<string, string> _pendingIdByIdentity = new(StringComparer.Ordinal);
    private readonly Dictionary<string, AiReviewState> _aiReviewByIdentity = new(StringComparer.Ordinal);
    private readonly HashSet<string> _sessionAllowlist = new(StringComparer.Ordinal);
    private readonly IConnectorApprovalStore _store;
    private readonly ICommandApprovalAiReviewer? _aiReviewer;
    private readonly TimeProvider _timeProvider;
    private readonly SemaphoreSlim _initializeGate = new(1, 1);
    private volatile bool _initialized;
    private ConnectorApprovalMode _mode = ConnectorApprovalMode.RequestApproval;
    private long _sessionGeneration;

    public CommandApprovalCoordinator(
        IConnectorApprovalStore store,
        TimeProvider? timeProvider = null) : this(store, null, timeProvider)
    {
    }

    public CommandApprovalCoordinator(
        IConnectorApprovalStore store,
        ICommandApprovalAiReviewer? aiReviewer,
        TimeProvider? timeProvider = null)
    {
        _store = store;
        _aiReviewer = aiReviewer;
        _timeProvider = timeProvider ?? TimeProvider.System;
    }

    public event EventHandler? PendingChanged;

    public event EventHandler<ConnectorApprovalDecisionEventArgs>? DecisionRecorded;

    public ConnectorApprovalMode Mode
    {
        get
        {
            lock (_gate)
            {
                return _mode;
            }
        }
    }

    public IReadOnlyList<ConnectorPendingApproval> Snapshot()
    {
        lock (_gate)
        {
            return _pendingById.Values
                .Select(value => value.Pending)
                .OrderBy(value => value.CreatedAt)
                .ToArray();
        }
    }

    public async Task InitializeAsync(CancellationToken cancellationToken = default)
    {
        if (_initialized)
        {
            return;
        }

        await _initializeGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_initialized)
            {
                return;
            }

            var storedMode = await _store.ReadModeAsync(cancellationToken).ConfigureAwait(false);
            lock (_gate)
            {
                _mode = storedMode ?? ConnectorApprovalMode.RequestApproval;
                _initialized = true;
            }
        }
        finally
        {
            _initializeGate.Release();
        }
    }

    public async Task SetModeAsync(
        ConnectorApprovalMode mode,
        bool fullControlRiskConfirmed = false,
        CancellationToken cancellationToken = default)
    {
        if (mode is ConnectorApprovalMode.FullControl && !fullControlRiskConfirmed)
        {
            throw new InvalidOperationException(
                "Full control requires an explicit risk confirmation.");
        }

        await InitializeAsync(cancellationToken).ConfigureAwait(false);
        await _store.SaveModeAsync(mode, cancellationToken).ConfigureAwait(false);
        lock (_gate)
        {
            _mode = mode;
        }
    }

    public async Task<ConnectorApprovalOutcome> RequestAsync(
        CommandApprovalRequest request,
        ConnectorApprovalRisk risk,
        CancellationToken cancellationToken = default)
    {
        Validate(request);
        await InitializeAsync(cancellationToken).ConfigureAwait(false);

        PendingState? pending = null;
        AiReviewState? aiReview = null;
        var startAiReview = false;
        ConnectorApprovalOutcome? immediate = null;
        string approvalId;
        lock (_gate)
        {
            approvalId = Guid.NewGuid().ToString("N");
            if (_sessionAllowlist.Contains(request.ScopeKey))
            {
                immediate = new ConnectorApprovalOutcome(
                    true,
                    _mode,
                    ConnectorApprovalReviewer.Session,
                    "This operation was approved for the current connector session.",
                    RememberedForSession: true);
            }
            else if (_mode is ConnectorApprovalMode.FullControl)
            {
                immediate = new ConnectorApprovalOutcome(
                    true,
                    _mode,
                    ConnectorApprovalReviewer.Policy,
                    "The full-control policy does not require per-command approval.");
            }
            else if (_pendingIdByIdentity.TryGetValue(request.StableIdentity, out var existingId) &&
                     _pendingById.TryGetValue(existingId, out var existing))
            {
                pending = existing;
                approvalId = existing.Pending.Id;
            }
            else if (_mode is ConnectorApprovalMode.AutoApproval && _aiReviewer is not null)
            {
                if (!_aiReviewByIdentity.TryGetValue(request.StableIdentity, out aiReview))
                {
                    aiReview = new AiReviewState(
                        approvalId,
                        request,
                        risk,
                        _mode,
                        _sessionGeneration);
                    _aiReviewByIdentity[request.StableIdentity] = aiReview;
                    startAiReview = true;
                }
                else
                {
                    approvalId = aiReview.ApprovalId;
                }
            }
            else
            {
                var reason = _mode is ConnectorApprovalMode.AutoApproval
                    ? "The Windows approval agent is not configured; user approval is required."
                    : risk.Reason;
                var pendingApproval = new ConnectorPendingApproval(
                    approvalId,
                    request.StableIdentity,
                    request.RequestId,
                    request.WorkspaceId,
                    request.DisplayCommand,
                    request.WorkingDirectory,
                    request.Source,
                    risk,
                    reason,
                    _mode,
                    _timeProvider.GetUtcNow(),
                    AvailableActions);
                pending = new PendingState(request, pendingApproval, _sessionGeneration);
                _pendingById[approvalId] = pending;
                _pendingIdByIdentity[request.StableIdentity] = approvalId;
            }
        }

        if (immediate is not null)
        {
            await AppendHistoryAsync(approvalId, request, risk, immediate, cancellationToken)
                .ConfigureAwait(false);
            RaiseDecisionRecorded(approvalId, request, risk, immediate);
            return immediate;
        }

        if (aiReview is not null)
        {
            if (startAiReview)
            {
                _ = ProcessAiReviewAsync(aiReview);
            }

            try
            {
                return await aiReview.Completion.Task.WaitAsync(cancellationToken).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
            {
                await CancelAiReviewAsync(aiReview).ConfigureAwait(false);
                throw;
            }
        }

        RaisePendingChanged();
        try
        {
            return await pending!.Completion.Task.WaitAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            try
            {
                await ResolveInternalAsync(
                    pending!.Pending.Id,
                    new ConnectorApprovalOutcome(
                        false,
                        pending.Pending.Mode,
                        ConnectorApprovalReviewer.System,
                        "Approval was cancelled before a decision was made."),
                    rememberForSession: false,
                    CancellationToken.None).ConfigureAwait(false);
            }
            catch
            {
                // Cancellation remains authoritative even when audit persistence is unavailable.
            }
            throw;
        }
    }

    public async Task<bool> ResolveAsync(
        string approvalId,
        ConnectorApprovalAction action,
        CancellationToken cancellationToken = default)
    {
        PendingState? state;
        lock (_gate)
        {
            _pendingById.TryGetValue(approvalId, out state);
        }

        if (state is null)
        {
            return false;
        }

        var approved = action is ConnectorApprovalAction.Accept or ConnectorApprovalAction.AcceptForSession;
        var outcome = new ConnectorApprovalOutcome(
            approved,
            state.Pending.Mode,
            ConnectorApprovalReviewer.User,
            action switch
            {
                ConnectorApprovalAction.AcceptForSession =>
                    "The user approved this operation for the current connector session.",
                ConnectorApprovalAction.Accept => "The user approved this operation.",
                _ => "The user declined this operation.",
            },
            action is ConnectorApprovalAction.AcceptForSession);
        return await ResolveInternalAsync(
            approvalId,
            outcome,
            action is ConnectorApprovalAction.AcceptForSession,
            cancellationToken).ConfigureAwait(false);
    }

    public async Task CancelAllAsync(
        string reason,
        CancellationToken cancellationToken = default)
    {
        PendingState[] states;
        AiReviewState[] reviews;
        lock (_gate)
        {
            states = _pendingById.Values.ToArray();
            reviews = _aiReviewByIdentity.Values.ToArray();
            _pendingById.Clear();
            _pendingIdByIdentity.Clear();
            _aiReviewByIdentity.Clear();
            _sessionAllowlist.Clear();
            _sessionGeneration++;
        }

        foreach (var review in reviews)
        {
            review.Lifetime.Cancel();
            var outcome = new ConnectorApprovalOutcome(
                false,
                review.Mode,
                ConnectorApprovalReviewer.System,
                reason);
            try
            {
                await AppendHistoryAsync(
                    review.ApprovalId,
                    review.Request,
                    review.Risk,
                    outcome,
                    cancellationToken).ConfigureAwait(false);
            }
            catch
            {
                // Disconnect cleanup must always release in-flight Relay work.
            }
            finally
            {
                review.Completion.TrySetResult(outcome);
                review.Lifetime.Dispose();
            }
        }

        foreach (var state in states)
        {
            var outcome = new ConnectorApprovalOutcome(
                false,
                state.Pending.Mode,
                ConnectorApprovalReviewer.System,
                reason);
            try
            {
                await AppendHistoryAsync(
                    state.Pending.Id,
                    state.Request,
                    state.Pending.Risk,
                    outcome,
                    cancellationToken).ConfigureAwait(false);
            }
            catch
            {
                // Disconnect cleanup must always release pending Relay work.
            }
            finally
            {
                state.Completion.TrySetResult(outcome);
            }
        }

        if (states.Length > 0 || reviews.Length > 0)
        {
            RaisePendingChanged();
        }
    }

    private async Task ProcessAiReviewAsync(AiReviewState state)
    {
        CommandApprovalAiReview review;
        try
        {
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(60));
            using var linked = CancellationTokenSource.CreateLinkedTokenSource(
                state.Lifetime.Token,
                timeout.Token);
            review = await _aiReviewer!.ReviewAsync(state.Request, state.Risk, linked.Token)
                .ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (state.Lifetime.IsCancellationRequested)
        {
            return;
        }
        catch (Exception exception)
        {
            review = new CommandApprovalAiReview(
                CommandApprovalAiDecisionKind.AskUser,
                SafeReviewerFailure(exception));
        }

        ConnectorApprovalOutcome? outcome = null;
        var pendingReason = string.Empty;
        lock (_gate)
        {
            if (!IsCurrentAiReview(state))
            {
                return;
            }

            if (_mode is not ConnectorApprovalMode.AutoApproval)
            {
                if (_sessionAllowlist.Contains(state.Request.ScopeKey))
                {
                    outcome = new ConnectorApprovalOutcome(
                        true,
                        _mode,
                        ConnectorApprovalReviewer.Session,
                        "This operation was approved for the current connector session.",
                        RememberedForSession: true);
                }
                else if (_mode is ConnectorApprovalMode.FullControl)
                {
                    outcome = new ConnectorApprovalOutcome(
                        true,
                        _mode,
                        ConnectorApprovalReviewer.Policy,
                        "The full-control policy does not require per-command approval.");
                }
                else
                {
                    pendingReason = state.Risk.Reason;
                }
            }
            else
            {
                switch (review.Decision)
                {
                    case CommandApprovalAiDecisionKind.Approve:
                        outcome = new ConnectorApprovalOutcome(
                            true,
                            _mode,
                            ConnectorApprovalReviewer.Ai,
                            review.Reason,
                            review.RememberForSession);
                        break;
                    case CommandApprovalAiDecisionKind.Deny:
                        outcome = new ConnectorApprovalOutcome(
                            false,
                            _mode,
                            ConnectorApprovalReviewer.Ai,
                            review.Reason);
                        break;
                    default:
                        pendingReason = review.Reason;
                        break;
                }
            }

            if (outcome is null)
            {
                TransitionAiReviewToPending(state, pendingReason);
            }
        }

        if (outcome is null)
        {
            RaisePendingChanged();
            return;
        }

        try
        {
            await AppendHistoryAsync(
                state.ApprovalId,
                state.Request,
                state.Risk,
                outcome,
                state.Lifetime.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (state.Lifetime.IsCancellationRequested)
        {
            return;
        }
        catch
        {
            lock (_gate)
            {
                if (IsCurrentAiReview(state))
                {
                    TransitionAiReviewToPending(
                        state,
                        "The automatic approval decision could not be audited; user approval is required.");
                }
            }
            RaisePendingChanged();
            return;
        }

        lock (_gate)
        {
            if (!IsCurrentAiReview(state))
            {
                return;
            }

            if (_mode != outcome.Mode)
            {
                TransitionAiReviewToPending(
                    state,
                    "The approval policy changed while the automatic review was running; user approval is required.");
                outcome = null;
            }
            else
            {
                _aiReviewByIdentity.Remove(state.Request.StableIdentity);
                if (outcome.Approved && outcome.RememberedForSession)
                {
                    _sessionAllowlist.Add(state.Request.ScopeKey);
                }
            }
        }

        if (outcome is null)
        {
            RaisePendingChanged();
            return;
        }

        state.Completion.TrySetResult(outcome);
        RaiseDecisionRecorded(state.ApprovalId, state.Request, state.Risk, outcome);
        state.Lifetime.Dispose();
    }

    private async Task CancelAiReviewAsync(AiReviewState state)
    {
        PendingState? pending = null;
        var reviewWasCurrent = false;
        lock (_gate)
        {
            if (IsCurrentAiReview(state))
            {
                _aiReviewByIdentity.Remove(state.Request.StableIdentity);
                reviewWasCurrent = true;
            }
            else if (_pendingById.TryGetValue(state.ApprovalId, out var candidate) &&
                     ReferenceEquals(candidate.Completion, state.Completion))
            {
                pending = candidate;
            }
        }

        if (pending is not null)
        {
            await ResolveInternalAsync(
                state.ApprovalId,
                new ConnectorApprovalOutcome(
                    false,
                    pending.Pending.Mode,
                    ConnectorApprovalReviewer.System,
                    "Approval was cancelled before a decision was made."),
                rememberForSession: false,
                CancellationToken.None).ConfigureAwait(false);
            return;
        }

        if (!reviewWasCurrent)
        {
            return;
        }

        state.Lifetime.Cancel();
        var outcome = new ConnectorApprovalOutcome(
            false,
            state.Mode,
            ConnectorApprovalReviewer.System,
            "Approval was cancelled before the automatic review completed.");
        try
        {
            await AppendHistoryAsync(
                state.ApprovalId,
                state.Request,
                state.Risk,
                outcome,
                CancellationToken.None).ConfigureAwait(false);
        }
        catch
        {
            // Caller cancellation remains authoritative when audit persistence is unavailable.
        }
        finally
        {
            state.Completion.TrySetResult(outcome);
            state.Lifetime.Dispose();
        }
    }

    private bool IsCurrentAiReview(AiReviewState state) =>
        state.SessionGeneration == _sessionGeneration &&
        _aiReviewByIdentity.TryGetValue(state.Request.StableIdentity, out var current) &&
        ReferenceEquals(current, state);

    private void TransitionAiReviewToPending(AiReviewState state, string? reason)
    {
        _aiReviewByIdentity.Remove(state.Request.StableIdentity);
        var pending = new ConnectorPendingApproval(
            state.ApprovalId,
            state.Request.StableIdentity,
            state.Request.RequestId,
            state.Request.WorkspaceId,
            state.Request.DisplayCommand,
            state.Request.WorkingDirectory,
            state.Request.Source,
            state.Risk,
            string.IsNullOrWhiteSpace(reason) ? state.Risk.Reason : reason,
            _mode,
            _timeProvider.GetUtcNow(),
            AvailableActions);
        var pendingState = new PendingState(
            state.Request,
            pending,
            state.SessionGeneration,
            state.Completion);
        _pendingById[state.ApprovalId] = pendingState;
        _pendingIdByIdentity[state.Request.StableIdentity] = state.ApprovalId;
        state.Lifetime.Dispose();
    }

    private static string SafeReviewerFailure(Exception _) =>
        "The automatic approval reviewer is unavailable; user approval is required.";

    public async Task<IReadOnlyList<ConnectorApprovalHistoryEntry>> ReadHistoryAsync(
        int limit = 1_000,
        CancellationToken cancellationToken = default) =>
        await _store.ReadHistoryAsync(limit, cancellationToken).ConfigureAwait(false);

    private async Task<bool> ResolveInternalAsync(
        string approvalId,
        ConnectorApprovalOutcome outcome,
        bool rememberForSession,
        CancellationToken cancellationToken)
    {
        PendingState? state;
        lock (_gate)
        {
            if (!_pendingById.TryGetValue(approvalId, out state) || !state.TryBeginResolution())
            {
                return false;
            }
        }

        try
        {
            await AppendHistoryAsync(
                approvalId,
                state.Request,
                state.Pending.Risk,
                outcome,
                cancellationToken).ConfigureAwait(false);
        }
        catch
        {
            state.CancelResolution();
            throw;
        }

        lock (_gate)
        {
            if (state.SessionGeneration != _sessionGeneration ||
                !_pendingById.TryGetValue(approvalId, out var current) ||
                !ReferenceEquals(current, state))
            {
                return false;
            }

            _pendingById.TryRemove(new KeyValuePair<string, PendingState>(approvalId, state));
            _pendingIdByIdentity.Remove(state.Pending.StableIdentity);
            if (rememberForSession && outcome.Approved)
            {
                _sessionAllowlist.Add(state.Request.ScopeKey);
            }
        }

        state.Completion.TrySetResult(outcome);
        RaisePendingChanged();
        return true;
    }

    private Task AppendHistoryAsync(
        string approvalId,
        CommandApprovalRequest request,
        ConnectorApprovalRisk risk,
        ConnectorApprovalOutcome outcome,
        CancellationToken cancellationToken) =>
        _store.AppendAsync(new ConnectorApprovalHistoryEntry(
            Guid.NewGuid().ToString("N"),
            approvalId,
            request.RequestId,
            request.WorkspaceId,
            request.DisplayCommand,
            request.WorkingDirectory,
            request.Source,
            outcome.Mode,
            outcome.Approved,
            outcome.Reviewer,
            risk.Level,
            risk.Reason,
            outcome.Reason,
            _timeProvider.GetUtcNow()), cancellationToken);

    private static void Validate(CommandApprovalRequest request)
    {
        if (string.IsNullOrWhiteSpace(request.RequestId) ||
            string.IsNullOrWhiteSpace(request.OwnerUserId) ||
            string.IsNullOrWhiteSpace(request.DeviceId) ||
            string.IsNullOrWhiteSpace(request.WorkspaceId) ||
            string.IsNullOrWhiteSpace(request.Command) ||
            string.IsNullOrWhiteSpace(request.WorkingDirectory) ||
            string.IsNullOrWhiteSpace(request.ScopeKey))
        {
            throw new ArgumentException("Command approval identity is incomplete.", nameof(request));
        }
    }

    private void RaisePendingChanged()
    {
        var handlers = PendingChanged;
        if (handlers is null)
        {
            return;
        }

        foreach (EventHandler handler in handlers.GetInvocationList())
        {
            try
            {
                handler(this, EventArgs.Empty);
            }
            catch
            {
                // A presentation subscriber cannot break the approval state machine.
            }
        }
    }

    private void RaiseDecisionRecorded(
        string approvalId,
        CommandApprovalRequest request,
        ConnectorApprovalRisk risk,
        ConnectorApprovalOutcome outcome)
    {
        var handlers = DecisionRecorded;
        if (handlers is null)
        {
            return;
        }

        var args = new ConnectorApprovalDecisionEventArgs(
            approvalId,
            request,
            risk,
            outcome,
            _timeProvider.GetUtcNow());
        foreach (EventHandler<ConnectorApprovalDecisionEventArgs> handler in handlers.GetInvocationList())
        {
            try
            {
                handler(this, args);
            }
            catch
            {
                // Presentation subscribers cannot break the approval state machine.
            }
        }
    }

    private sealed class PendingState(
        CommandApprovalRequest request,
        ConnectorPendingApproval pending,
        long sessionGeneration,
        TaskCompletionSource<ConnectorApprovalOutcome>? completion = null)
    {
        public CommandApprovalRequest Request { get; } = request;

        public ConnectorPendingApproval Pending { get; } = pending;

        public long SessionGeneration { get; } = sessionGeneration;

        public TaskCompletionSource<ConnectorApprovalOutcome> Completion { get; } = completion ??
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        private int _resolving;

        public bool TryBeginResolution() =>
            Interlocked.CompareExchange(ref _resolving, 1, 0) == 0;

        public void CancelResolution() => Volatile.Write(ref _resolving, 0);
    }

    private sealed class AiReviewState(
        string approvalId,
        CommandApprovalRequest request,
        ConnectorApprovalRisk risk,
        ConnectorApprovalMode mode,
        long sessionGeneration)
    {
        public string ApprovalId { get; } = approvalId;
        public CommandApprovalRequest Request { get; } = request;
        public ConnectorApprovalRisk Risk { get; } = risk;
        public ConnectorApprovalMode Mode { get; } = mode;
        public long SessionGeneration { get; } = sessionGeneration;
        public CancellationTokenSource Lifetime { get; } = new();
        public TaskCompletionSource<ConnectorApprovalOutcome> Completion { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
    }
}
