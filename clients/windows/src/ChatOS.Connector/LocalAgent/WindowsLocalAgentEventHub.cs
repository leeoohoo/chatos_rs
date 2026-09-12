using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed record WindowsLocalAgentEventDrainResult(
    ulong InitialSequence,
    ulong AcknowledgedSequence,
    int AppliedEventCount);

public interface IWindowsLocalAgentEventHub
{
    Task StartAsync(string accountId, CancellationToken cancellationToken = default);
    Task StopAsync();
    Task<WindowsLocalAgentEventDrainResult> DrainAvailableAsync(
        string accountId,
        CancellationToken cancellationToken = default);
}

/// One account-level event pump. It is independent from page lifetime and
/// resolves a fresh IPC client for each drain so Host restart endpoints are
/// never cached.
public sealed class WindowsLocalAgentEventHub : IWindowsLocalAgentEventHub
{
    private const uint PageLimit = 500;
    private static readonly TimeSpan IdleDelay = TimeSpan.FromMilliseconds(350);
    private readonly IWindowsLocalAgentAccountSession _accountSession;
    private readonly IWindowsLocalAgentProjectionStore _store;
    private readonly SemaphoreSlim _gate = new(1, 1);
    private CancellationTokenSource? _lifetime;
    private Task? _worker;
    private string? _accountId;
    private Exception? _lastFailure;

    public Exception? LastFailure => Volatile.Read(ref _lastFailure);

    public WindowsLocalAgentEventHub(
        IWindowsLocalAgentAccountSession accountSession,
        IWindowsLocalAgentProjectionStore store)
    {
        _accountSession = accountSession;
        _store = store;
    }

    public async Task StartAsync(string accountId, CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(accountId);
        await StopAsync().ConfigureAwait(false);
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            _accountId = accountId;
            Volatile.Write(ref _lastFailure, null);
            _lifetime = new CancellationTokenSource();
            _worker = RunLoopAsync(accountId, _lifetime.Token);
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task StopAsync()
    {
        CancellationTokenSource? lifetime;
        Task? worker;
        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            lifetime = _lifetime;
            worker = _worker;
            _lifetime = null;
            _worker = null;
            _accountId = null;
        }
        finally
        {
            _gate.Release();
        }
        if (lifetime is not null)
        {
            lifetime.Cancel();
            try { if (worker is not null) await worker.ConfigureAwait(false); }
            catch (OperationCanceledException) { }
            lifetime.Dispose();
        }
    }

    public async Task<WindowsLocalAgentEventDrainResult> DrainAvailableAsync(
        string accountId,
        CancellationToken cancellationToken = default)
    {
        var client = await _accountSession.GetClientAsync(accountId, cancellationToken)
            .ConfigureAwait(false);
        var initial = await client.GetUIEventCursorAsync(cancellationToken).ConfigureAwait(false);
        var cursor = initial;
        var applied = 0;
        while (true)
        {
            var page = await client.SubscribeRunEventsAsync(cursor, PageLimit, cancellationToken)
                .ConfigureAwait(false);
            ValidatePage(page, cursor);
            if (page.Events.Count == 0)
            {
                return new WindowsLocalAgentEventDrainResult(initial, cursor, applied);
            }

            var resolved = new List<WindowsLocalAgentResolvedEvent>(page.Events.Count);
            foreach (var item in page.Events)
            {
                resolved.Add(await ResolveAsync(accountId, item, client, cancellationToken)
                    .ConfigureAwait(false));
            }
            await _store.ApplyPageAsync(accountId, resolved, cancellationToken)
                .ConfigureAwait(false);
            var through = page.Events[^1].EventSeq;
            var acknowledged = await client.AcknowledgeUIEventsAsync(through, cancellationToken)
                .ConfigureAwait(false);
            if (acknowledged != through)
            {
                throw new InvalidDataException(
                    $"Local Agent cursor acknowledgement mismatch ({through} != {acknowledged}).");
            }
            await _store.MarkAcknowledgedAsync(accountId, acknowledged, cancellationToken)
                .ConfigureAwait(false);
            cursor = acknowledged;
            applied += page.Events.Count;
            if (!page.HasMore)
            {
                return new WindowsLocalAgentEventDrainResult(initial, cursor, applied);
            }
        }
    }

    private async Task RunLoopAsync(string accountId, CancellationToken cancellationToken)
    {
        var failures = 0;
        while (!cancellationToken.IsCancellationRequested)
        {
            try
            {
                var result = await DrainAvailableAsync(accountId, cancellationToken)
                    .ConfigureAwait(false);
                failures = 0;
                if (result.AppliedEventCount == 0)
                {
                    await Task.Delay(IdleDelay, cancellationToken).ConfigureAwait(false);
                }
            }
            catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
            {
                return;
            }
            catch (Exception error) when (
                WindowsLocalAgentHostFailurePolicy.IsTransientEndpointLoss(error))
            {
                failures = Math.Min(failures + 1, 5);
                await Task.Delay(TimeSpan.FromSeconds(1 << (failures - 1)), cancellationToken)
                    .ConfigureAwait(false);
            }
            catch (Exception error)
            {
                Volatile.Write(ref _lastFailure, error);
                return;
            }
        }
    }

    private static async Task<WindowsLocalAgentResolvedEvent> ResolveAsync(
        string accountId,
        LocalAgentUIEvent item,
        ILocalAgentIPCClient client,
        CancellationToken cancellationToken)
    {
        var runId = RunId(item.Event);
        if (runId is null)
        {
            return new WindowsLocalAgentResolvedEvent(item, null, null, null);
        }
        LocalAgentRunSnapshot run;
        if (item.Event.Type == "run_snapshot")
        {
            run = item.Event.Payload?.Deserialize<LocalAgentRunSnapshot>(
                WindowsLocalAgentIPCClient.ProtocolJsonOptions)
                ?? throw new InvalidDataException("Local Agent Run event payload is empty.");
        }
        else
        {
            run = await client.GetRunAsync(runId, cancellationToken).ConfigureAwait(false);
        }
        if (run.RunId != runId || run.OwnerUserId != accountId)
        {
            throw new InvalidDataException("Local Agent event resolved to a different Run identity.");
        }

        LocalAgentTaskSnapshot? task = null;
        LocalAgentMainChatRunBinding? binding = null;
        if (run.ProfileKey == "task_runner")
        {
            if (run.OwnerEntityType != "task")
            {
                throw new InvalidDataException("Task Runner event has an invalid owner.");
            }
            task = await client.GetTaskAsync(run.OwnerEntityId, cancellationToken)
                .ConfigureAwait(false);
            if (task.TaskId != run.OwnerEntityId
                || !task.RunIds.Contains(run.RunId, StringComparer.Ordinal)
                || task.ProjectId != run.ProjectId)
            {
                throw new InvalidDataException("Task Runner event has an inconsistent Task identity.");
            }
        }
        else if (run.ProfileKey == "main_chat")
        {
            if (run.OwnerEntityType != "conversation")
            {
                throw new InvalidDataException("Main Chat event has an invalid owner.");
            }
            binding = await client.GetMainChatRunBindingAsync(run.RunId, cancellationToken)
                .ConfigureAwait(false);
            WindowsLocalAgentStartupRecovery.ValidateBinding(run, binding);
        }
        return new WindowsLocalAgentResolvedEvent(item, run, task, binding);
    }

    private static string? RunId(LocalAgentTaggedValue value)
    {
        if (value.Type == "host_status") return null;
        if (value.Type is not ("run_snapshot" or "model_stream" or "tool_snapshot"
            or "user_interaction" or "memory_sync"))
        {
            throw new InvalidDataException($"Unknown Local Agent UI event type '{value.Type}'.");
        }
        if (value.Payload is not { ValueKind: JsonValueKind.Object } payload
            || !payload.TryGetProperty("run_id", out var runId)
            || runId.ValueKind != JsonValueKind.String
            || string.IsNullOrWhiteSpace(runId.GetString()))
        {
            throw new InvalidDataException("Local Agent UI event has no run_id.");
        }
        return runId.GetString();
    }

    private static void ValidatePage(LocalAgentEventPage page, ulong after)
    {
        if (page.Events.Count == 0)
        {
            if (page.NextSequence != after || page.HasMore)
            {
                throw new InvalidDataException("Empty Local Agent event page is inconsistent.");
            }
            return;
        }
        var previous = after;
        foreach (var item in page.Events)
        {
            if (item.EventSeq <= previous)
            {
                throw new InvalidDataException("Local Agent event sequence is not strictly increasing.");
            }
            previous = item.EventSeq;
        }
        if (page.NextSequence != previous)
        {
            throw new InvalidDataException("Local Agent event next sequence is inconsistent.");
        }
    }
}
