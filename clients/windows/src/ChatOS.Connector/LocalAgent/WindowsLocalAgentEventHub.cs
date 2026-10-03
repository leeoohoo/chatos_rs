using System.Runtime.CompilerServices;
using System.Threading.Channels;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalAgentEventUpdate(
    string OwnerUserId,
    bool IsReconcile,
    IReadOnlyList<WindowsLocalAgentEvent> Events);

/// <summary>
/// Owns one durable Host event cursor per account and fans wake-ups out to all UI consumers.
/// Consumers always reconcile authoritative SQLite projections; this stream is not a second store.
/// </summary>
public sealed class WindowsLocalAgentEventHub
{
    private readonly WindowsLocalAgentRuntimeClient _runtime;
    private readonly object _gate = new();
    private readonly Dictionary<Guid, Channel<WindowsLocalAgentEventUpdate>> _subscribers = [];
    private string? _ownerUserId;
    private ulong _generation;
    private long? _cursor;
    private CancellationTokenSource? _pollingCancellation;
    private Task? _pollingTask;

    public WindowsLocalAgentEventHub(WindowsLocalAgentRuntimeClient runtime)
    {
        _runtime = runtime;
    }

    public void Configure(string ownerUserId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(ownerUserId);
        List<Channel<WindowsLocalAgentEventUpdate>> subscribers;
        lock (_gate)
        {
            if (!string.Equals(_ownerUserId, ownerUserId, StringComparison.Ordinal))
            {
                StopPollingLocked();
                _ownerUserId = ownerUserId;
                _cursor = null;
                _generation++;
            }
            subscribers = _subscribers.Values.ToList();
            StartPollingLocked();
        }
        Broadcast(subscribers, Reconcile(ownerUserId));
    }

    public void Reset()
    {
        lock (_gate)
        {
            StopPollingLocked();
            _ownerUserId = null;
            _cursor = null;
            _generation++;
        }
    }

    internal async IAsyncEnumerable<WindowsLocalAgentEventUpdate> UpdatesAsync(
        [EnumeratorCancellation] CancellationToken cancellationToken = default)
    {
        var id = Guid.NewGuid();
        var channel = Channel.CreateBounded<WindowsLocalAgentEventUpdate>(new BoundedChannelOptions(64)
        {
            SingleReader = true,
            SingleWriter = false,
            FullMode = BoundedChannelFullMode.DropOldest,
        });
        lock (_gate)
        {
            _subscribers.Add(id, channel);
            if (_ownerUserId is { } owner) channel.Writer.TryWrite(Reconcile(owner));
            StartPollingLocked();
        }
        try
        {
            await foreach (var update in channel.Reader.ReadAllAsync(cancellationToken)
                .ConfigureAwait(false))
            {
                yield return update;
            }
        }
        finally
        {
            lock (_gate)
            {
                _subscribers.Remove(id);
                channel.Writer.TryComplete();
                if (_subscribers.Count == 0)
                {
                    StopPollingLocked();
                    _cursor = null;
                }
            }
        }
    }

    private void StartPollingLocked()
    {
        if (_ownerUserId is null || _subscribers.Count == 0 ||
            _pollingTask is { IsCompleted: false }) return;
        var cancellation = new CancellationTokenSource();
        var generation = _generation;
        _pollingCancellation = cancellation;
        _pollingTask = Task.Run(() => PollAsync(generation, cancellation));
    }

    private void StopPollingLocked()
    {
        _pollingCancellation?.Cancel();
        _pollingCancellation = null;
        _pollingTask = null;
    }

    private async Task PollAsync(ulong generation, CancellationTokenSource source)
    {
        try
        {
            while (!source.IsCancellationRequested)
            {
                string owner;
                long? cursor;
                lock (_gate)
                {
                    if (_generation != generation || _subscribers.Count == 0 ||
                        _ownerUserId is not { } configuredOwner) return;
                    owner = configuredOwner;
                    cursor = _cursor;
                }

                try
                {
                    if (cursor is null)
                    {
                        cursor = await _runtime.GetLatestEventCursorAsync(owner, source.Token)
                            .ConfigureAwait(false);
                        lock (_gate)
                        {
                            if (!IsCurrentLocked(owner, generation)) continue;
                            _cursor = cursor;
                        }
                    }

                    var page = await _runtime.WaitEventsAsync(
                        owner, cursor.Value, timeoutMs: 20_000, payloadMode: "routing",
                        cancellationToken: source.Token).ConfigureAwait(false);
                    List<Channel<WindowsLocalAgentEventUpdate>> subscribers;
                    lock (_gate)
                    {
                        if (!IsCurrentLocked(owner, generation)) continue;
                        _cursor = Math.Max(cursor.Value, page.NextCursor);
                        subscribers = _subscribers.Values.ToList();
                    }
                    if (page.Events.Count > 0)
                    {
                        Broadcast(subscribers, new(owner, false, page.Events));
                    }
                }
                catch (OperationCanceledException) when (source.IsCancellationRequested)
                {
                    return;
                }
                catch
                {
                    await Task.Delay(TimeSpan.FromSeconds(1), source.Token).ConfigureAwait(false);
                }
            }
        }
        catch (OperationCanceledException)
        {
        }
        finally
        {
            lock (_gate)
            {
                if (_pollingCancellation == source)
                {
                    _pollingCancellation = null;
                    _pollingTask = null;
                    StartPollingLocked();
                }
            }
            source.Dispose();
        }
    }

    private bool IsCurrentLocked(string ownerUserId, ulong generation) =>
        _generation == generation && _subscribers.Count > 0 &&
        string.Equals(_ownerUserId, ownerUserId, StringComparison.Ordinal);

    private static WindowsLocalAgentEventUpdate Reconcile(string ownerUserId) =>
        new(ownerUserId, true, []);

    private static void Broadcast(
        IEnumerable<Channel<WindowsLocalAgentEventUpdate>> subscribers,
        WindowsLocalAgentEventUpdate update)
    {
        foreach (var subscriber in subscribers) subscriber.Writer.TryWrite(update);
    }
}
