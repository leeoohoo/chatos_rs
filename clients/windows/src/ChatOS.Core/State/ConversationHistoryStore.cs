using ChatOS.Core.Domain;

namespace ChatOS.Core.State;

public sealed class ConversationHistoryStore
{
    private readonly object _gate = new();
    private readonly Dictionary<string, SessionState> _sessions = new(StringComparer.Ordinal);

    public void MergeCachedTurns(
        IReadOnlyList<ConversationTurn> turns,
        string conversationId)
    {
        lock (_gate)
        {
            var state = StateFor(conversationId);
            Merge(turns, conversationId, state);
        }
    }

    public void MergePage(
        HistoryPage page,
        string conversationId,
        ConversationHistoryPageOrigin origin = ConversationHistoryPageOrigin.Latest)
    {
        lock (_gate)
        {
            var state = StateFor(conversationId);
            var changed = Merge(page.Turns, conversationId, state);
            if (changed &&
                origin == ConversationHistoryPageOrigin.Latest &&
                state.ViewportAnchor?.IsPinnedToBottom == false)
            {
                state.UnreadNewerCount++;
            }

            if (origin == ConversationHistoryPageOrigin.Latest)
            {
                if (page.RequestGeneration >= state.NewestAcceptedLatestGeneration)
                {
                    state.NewestAcceptedLatestGeneration = page.RequestGeneration;
                    if (!state.HasLoadedOlderPage)
                    {
                        state.OlderCursor = page.OlderCursor;
                        state.HasOlder = page.HasOlder;
                    }
                }
            }
            else if (page.RequestGeneration >= state.NewestAcceptedOlderGeneration)
            {
                state.NewestAcceptedOlderGeneration = page.RequestGeneration;
                state.HasLoadedOlderPage = true;
                state.OlderCursor = page.OlderCursor;
                state.HasOlder = page.HasOlder;
            }

            state.SnapshotRevision = Math.Max(state.SnapshotRevision, page.SnapshotRevision);
        }
    }

    public void ApplyRealtime(RealtimeTurnEvent turnEvent, bool userIsReadingOlderContent)
    {
        lock (_gate)
        {
            var conversationId = turnEvent.Turn.ConversationId;
            var state = StateFor(conversationId);
            if (!state.AppliedEventIds.Add(turnEvent.EventId))
            {
                return;
            }

            state.LastAppliedEventSequence = Math.Max(
                state.LastAppliedEventSequence,
                turnEvent.EventSequence);
            var changed = Merge(new[] { turnEvent.Turn }, conversationId, state);
            if (changed && userIsReadingOlderContent)
            {
                state.UnreadNewerCount++;
            }
        }
    }

    public void SetViewportAnchor(ViewportAnchor? anchor, string conversationId)
    {
        lock (_gate)
        {
            var state = StateFor(conversationId);
            state.ViewportAnchor = anchor;
            if (anchor?.IsPinnedToBottom == true)
            {
                state.UnreadNewerCount = 0;
            }
        }
    }

    public void MarkNewerContentRead(string conversationId)
    {
        lock (_gate)
        {
            StateFor(conversationId).UnreadNewerCount = 0;
        }
    }

    public void DiscardOptimisticTurn(string conversationId, string turnId)
    {
        lock (_gate)
        {
            var state = StateFor(conversationId);
            if (state.TurnsById.TryGetValue(turnId, out var turn) && turn.Revision == 0)
            {
                state.TurnsById.Remove(turnId);
            }
        }
    }

    public ConversationHistorySnapshot Snapshot(string conversationId)
    {
        lock (_gate)
        {
            var state = StateFor(conversationId);
            return new ConversationHistorySnapshot(
                conversationId,
                state.TurnsById.Values
                    .OrderBy(static turn => turn, ConversationTurnComparer.Instance)
                    .ToArray(),
                state.OlderCursor,
                state.HasOlder,
                state.SnapshotRevision,
                state.ViewportAnchor,
                state.UnreadNewerCount);
        }
    }

    public void Clear(string conversationId)
    {
        lock (_gate)
        {
            _sessions.Remove(conversationId);
        }
    }

    private SessionState StateFor(string conversationId)
    {
        if (!_sessions.TryGetValue(conversationId, out var state))
        {
            state = new SessionState();
            _sessions[conversationId] = state;
        }

        return state;
    }

    private static bool Merge(
        IReadOnlyList<ConversationTurn> incomingTurns,
        string conversationId,
        SessionState state)
    {
        var changed = false;
        var newlySuperseded = incomingTurns
            .Select(static turn => turn.ProjectExecutionContext?.ReplacedExecutionGroupId?.Trim())
            .Where(static value => !string.IsNullOrEmpty(value))
            .OfType<string>()
            .ToArray();
        if (newlySuperseded.Length > 0)
        {
            state.SupersededExecutionGroupIds.UnionWith(newlySuperseded);
            foreach (var pair in state.TurnsById.ToArray())
            {
                if (state.SupersededExecutionGroupIds.Contains(ExecutionGroupIdentity(pair.Value)) &&
                    pair.Value.IsTaskGraphAvailable)
                {
                    state.TurnsById[pair.Key] = pair.Value with { IsTaskGraphAvailable = false };
                    changed = true;
                }
            }
        }

        foreach (var incoming in incomingTurns)
        {
            if (!string.Equals(incoming.ConversationId, conversationId, StringComparison.Ordinal))
            {
                continue;
            }

            var turn = state.SupersededExecutionGroupIds.Contains(ExecutionGroupIdentity(incoming))
                ? incoming with { IsTaskGraphAvailable = false }
                : incoming;
            if (!state.TurnsById.TryGetValue(turn.Id, out var existing))
            {
                state.TurnsById[turn.Id] = turn;
                changed = true;
            }
            else if (turn.Revision > existing.Revision)
            {
                state.TurnsById[turn.Id] = !existing.IsTaskGraphAvailable
                    ? turn with { IsTaskGraphAvailable = false }
                    : turn;
                changed = true;
            }
        }

        return changed;
    }

    private static string ExecutionGroupIdentity(ConversationTurn turn) =>
        string.IsNullOrWhiteSpace(turn.ProjectExecutionContext?.ExecutionGroupId)
            ? turn.UserMessage.Id
            : turn.ProjectExecutionContext.ExecutionGroupId.Trim();

    private sealed class SessionState
    {
        public Dictionary<string, ConversationTurn> TurnsById { get; } = new(StringComparer.Ordinal);

        public string? OlderCursor { get; set; }

        public bool HasOlder { get; set; }

        public long SnapshotRevision { get; set; }

        public long NewestAcceptedLatestGeneration { get; set; }

        public long NewestAcceptedOlderGeneration { get; set; }

        public bool HasLoadedOlderPage { get; set; }

        public long LastAppliedEventSequence { get; set; }

        public HashSet<string> AppliedEventIds { get; } = new(StringComparer.Ordinal);

        public ViewportAnchor? ViewportAnchor { get; set; }

        public int UnreadNewerCount { get; set; }

        public HashSet<string> SupersededExecutionGroupIds { get; } = new(StringComparer.Ordinal);
    }

    private sealed class ConversationTurnComparer : IComparer<ConversationTurn>
    {
        public static ConversationTurnComparer Instance { get; } = new();

        public int Compare(ConversationTurn? left, ConversationTurn? right)
        {
            if (ReferenceEquals(left, right)) return 0;
            if (left is null) return -1;
            if (right is null) return 1;

            var leftHasDate = left.StartedAt != DateTimeOffset.MinValue;
            var rightHasDate = right.StartedAt != DateTimeOffset.MinValue;
            if (leftHasDate && rightHasDate && left.StartedAt != right.StartedAt)
            {
                return left.StartedAt.CompareTo(right.StartedAt);
            }

            var sequence = left.Sequence.CompareTo(right.Sequence);
            return sequence != 0
                ? sequence
                : string.Compare(left.Id, right.Id, StringComparison.Ordinal);
        }
    }
}
