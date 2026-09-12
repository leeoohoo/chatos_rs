using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed record WindowsLocalAgentRecoveredRun(
    LocalAgentRunSnapshot Run,
    LocalAgentRunDetail? Detail,
    LocalAgentMainChatRunBinding? MainChatBinding,
    ulong SnapshotEventSequence = 0);

public sealed record WindowsLocalAgentProjectionSnapshot(
    string AccountId,
    IReadOnlyDictionary<string, WindowsLocalAgentRecoveredRun> Runs,
    IReadOnlyDictionary<string, LocalAgentTaskSnapshot> Tasks,
    ulong AcknowledgedEventSequence,
    ulong LastAppliedEventSequence);

public sealed record WindowsLocalAgentResolvedEvent(
    LocalAgentUIEvent Event,
    LocalAgentRunSnapshot? Run,
    LocalAgentTaskSnapshot? Task,
    LocalAgentMainChatRunBinding? MainChatBinding);

public interface IWindowsLocalAgentProjectionStore
{
    event EventHandler<WindowsLocalAgentProjectionSnapshot>? Changed;
    event EventHandler? Cleared;

    Task ReplaceAsync(
        WindowsLocalAgentProjectionSnapshot snapshot,
        CancellationToken cancellationToken = default);

    Task ApplyPageAsync(
        string accountId,
        IReadOnlyList<WindowsLocalAgentResolvedEvent> events,
        CancellationToken cancellationToken = default);

    Task MarkAcknowledgedAsync(
        string accountId,
        ulong throughSequence,
        CancellationToken cancellationToken = default);

    Task<WindowsLocalAgentProjectionSnapshot?> GetAsync(
        CancellationToken cancellationToken = default);

    Task ResetAsync(CancellationToken cancellationToken = default);
}

/// Account-scoped Windows presentation projection. Durable authority remains
/// in the shared Rust Host; this store only publishes immutable native UI
/// snapshots and deduplicates replay before the Host cursor is acknowledged.
public sealed class WindowsLocalAgentProjectionStore : IWindowsLocalAgentProjectionStore
{
    private readonly SemaphoreSlim _gate = new(1, 1);
    private WindowsLocalAgentProjectionSnapshot? _snapshot;

    public event EventHandler<WindowsLocalAgentProjectionSnapshot>? Changed;
    public event EventHandler? Cleared;

    public async Task ReplaceAsync(
        WindowsLocalAgentProjectionSnapshot snapshot,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(snapshot);
        ValidateIdentity(snapshot.AccountId);
        if (snapshot.LastAppliedEventSequence < snapshot.AcknowledgedEventSequence)
        {
            throw new InvalidDataException("Local Agent projection event cursor is invalid.");
        }
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            _snapshot = Clone(snapshot);
        }
        finally
        {
            _gate.Release();
        }
        Changed?.Invoke(this, Clone(snapshot));
    }

    public async Task ApplyPageAsync(
        string accountId,
        IReadOnlyList<WindowsLocalAgentResolvedEvent> events,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(events);
        if (events.Count == 0) return;
        WindowsLocalAgentProjectionSnapshot updated;
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var current = RequireAccount(accountId);
            var previous = current.AcknowledgedEventSequence;
            foreach (var item in events)
            {
                if (item.Event.EventSeq <= previous)
                {
                    throw new InvalidDataException("Local Agent event page is not strictly ordered.");
                }
                previous = item.Event.EventSeq;
            }

            var runs = current.Runs.ToDictionary(pair => pair.Key, pair => pair.Value,
                StringComparer.Ordinal);
            var tasks = current.Tasks.ToDictionary(pair => pair.Key, pair => pair.Value,
                StringComparer.Ordinal);
            foreach (var item in events.Where(item =>
                         item.Event.EventSeq > current.LastAppliedEventSequence))
            {
                if (item.Run is { } run)
                {
                    var previousRun = runs.GetValueOrDefault(run.RunId);
                    if (previousRun is null
                        || item.Event.EventSeq > previousRun.SnapshotEventSequence)
                    {
                        runs[run.RunId] = new WindowsLocalAgentRecoveredRun(
                            run,
                            previousRun?.Detail,
                            item.MainChatBinding ?? previousRun?.MainChatBinding,
                            previousRun?.SnapshotEventSequence ?? 0);
                    }
                }
                if (item.Task is { } task)
                {
                    tasks[task.TaskId] = task;
                }
            }
            updated = new WindowsLocalAgentProjectionSnapshot(
                accountId,
                runs,
                tasks,
                current.AcknowledgedEventSequence,
                Math.Max(current.LastAppliedEventSequence, events[^1].Event.EventSeq));
            _snapshot = updated;
        }
        finally
        {
            _gate.Release();
        }
        Changed?.Invoke(this, Clone(updated));
    }

    public async Task MarkAcknowledgedAsync(
        string accountId,
        ulong throughSequence,
        CancellationToken cancellationToken = default)
    {
        WindowsLocalAgentProjectionSnapshot updated;
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var current = RequireAccount(accountId);
            if (throughSequence < current.AcknowledgedEventSequence
                || throughSequence > current.LastAppliedEventSequence)
            {
                throw new InvalidDataException("Local Agent acknowledged cursor is invalid.");
            }
            updated = current with { AcknowledgedEventSequence = throughSequence };
            _snapshot = updated;
        }
        finally
        {
            _gate.Release();
        }
        Changed?.Invoke(this, Clone(updated));
    }

    public async Task<WindowsLocalAgentProjectionSnapshot?> GetAsync(
        CancellationToken cancellationToken = default)
    {
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            return _snapshot is null ? null : Clone(_snapshot);
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task ResetAsync(CancellationToken cancellationToken = default)
    {
        var changed = false;
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            changed = _snapshot is not null;
            _snapshot = null;
        }
        finally
        {
            _gate.Release();
        }
        if (changed) Cleared?.Invoke(this, EventArgs.Empty);
    }

    private WindowsLocalAgentProjectionSnapshot RequireAccount(string accountId)
    {
        ValidateIdentity(accountId);
        if (_snapshot is null)
        {
            throw new InvalidOperationException("Local Agent projection has not been restored.");
        }
        if (!string.Equals(_snapshot.AccountId, accountId, StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Local Agent projection account mismatch.");
        }
        return _snapshot;
    }

    private static WindowsLocalAgentProjectionSnapshot Clone(
        WindowsLocalAgentProjectionSnapshot value) => value with
    {
        Runs = value.Runs.ToDictionary(pair => pair.Key, pair => pair.Value,
            StringComparer.Ordinal),
        Tasks = value.Tasks.ToDictionary(pair => pair.Key, pair => pair.Value,
            StringComparer.Ordinal),
    };

    private static void ValidateIdentity(string value)
    {
        if (string.IsNullOrWhiteSpace(value) || value != value.Trim() || value.Any(char.IsControl))
        {
            throw new ArgumentException("Local Agent account identity is invalid.", nameof(value));
        }
    }
}
