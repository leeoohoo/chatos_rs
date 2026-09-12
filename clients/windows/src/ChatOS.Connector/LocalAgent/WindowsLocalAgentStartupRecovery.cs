using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public interface IWindowsLocalAgentStartupRecovery
{
    Task RestoreAsync(
        string accountId,
        ILocalAgentIPCClient client,
        CancellationToken cancellationToken = default);
}

public sealed class WindowsLocalAgentStartupRecovery : IWindowsLocalAgentStartupRecovery
{
    private const uint PageLimit = 500;
    private readonly IWindowsLocalAgentProjectionStore _store;

    public WindowsLocalAgentStartupRecovery(IWindowsLocalAgentProjectionStore store)
    {
        _store = store;
    }

    public async Task RestoreAsync(
        string accountId,
        ILocalAgentIPCClient client,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(accountId);
        ArgumentNullException.ThrowIfNull(client);
        var runs = await AllRunsAsync(client, cancellationToken).ConfigureAwait(false);
        var tasks = await AllTasksAsync(client, cancellationToken).ConfigureAwait(false);
        ValidateAuthority(accountId, runs, tasks);

        var recovered = new Dictionary<string, WindowsLocalAgentRecoveredRun>(StringComparer.Ordinal);
        foreach (var run in runs)
        {
            var detail = await CompleteDetailAsync(client, run.RunId, cancellationToken)
                .ConfigureAwait(false);
            ValidateRunDetail(run, detail.Run);
            var authoritativeRun = detail.Run;
            LocalAgentMainChatRunBinding? binding = null;
            if (authoritativeRun.ProfileKey == "main_chat")
            {
                if (authoritativeRun.OwnerEntityType != "conversation")
                {
                    throw Invalid($"Main Chat Run '{authoritativeRun.RunId}' has an invalid owner.");
                }
                binding = await client.GetMainChatRunBindingAsync(authoritativeRun.RunId, cancellationToken)
                    .ConfigureAwait(false);
                ValidateBinding(authoritativeRun, binding);
            }
            recovered.Add(authoritativeRun.RunId, new WindowsLocalAgentRecoveredRun(
                authoritativeRun,
                detail,
                binding,
                detail.SnapshotEventSequence));
        }
        var cursor = await client.GetUIEventCursorAsync(cancellationToken).ConfigureAwait(false);
        await _store.ReplaceAsync(
            new WindowsLocalAgentProjectionSnapshot(
                accountId,
                recovered,
                tasks.ToDictionary(task => task.TaskId, StringComparer.Ordinal),
                cursor,
                cursor),
            cancellationToken).ConfigureAwait(false);
    }

    private static async Task<IReadOnlyList<LocalAgentRunSnapshot>> AllRunsAsync(
        ILocalAgentIPCClient client,
        CancellationToken cancellationToken) => await AllPagesAsync(
            async cursor =>
            {
                var page = await client.ListRunsAsync(cursor, PageLimit, cancellationToken)
                    .ConfigureAwait(false);
                return (page.Runs, page.NextCursor);
            },
            run => run.RunId,
            "Run").ConfigureAwait(false);

    private static async Task<IReadOnlyList<LocalAgentTaskSnapshot>> AllTasksAsync(
        ILocalAgentIPCClient client,
        CancellationToken cancellationToken) => await AllPagesAsync(
            async cursor =>
            {
                var page = await client.ListTasksAsync(cursor, PageLimit, cancellationToken)
                    .ConfigureAwait(false);
                return (page.Tasks, page.NextCursor);
            },
            task => task.TaskId,
            "Task").ConfigureAwait(false);

    private static async Task<IReadOnlyList<T>> AllPagesAsync<T>(
        Func<string?, Task<(IReadOnlyList<T> Values, string? NextCursor)>> load,
        Func<T, string> identity,
        string kind)
    {
        string? cursor = null;
        var values = new List<T>();
        var ids = new HashSet<string>(StringComparer.Ordinal);
        do
        {
            var page = await load(cursor).ConfigureAwait(false);
            foreach (var value in page.Values)
            {
                var id = identity(value);
                if (!ids.Add(id)) throw Invalid($"Duplicate {kind} '{id}'.");
                values.Add(value);
            }
            ValidateCursor(cursor, page.NextCursor, kind);
            cursor = page.NextCursor;
        } while (cursor is not null);
        return values;
    }

    private static async Task<LocalAgentRunDetail> CompleteDetailAsync(
        ILocalAgentIPCClient client,
        string runId,
        CancellationToken cancellationToken)
    {
        uint offset = 0;
        var events = new List<LocalAgentRunTimelineEvent>();
        var eventIds = new HashSet<string>(StringComparer.Ordinal);
        LocalAgentRunDetail? latest = null;
        ulong snapshotSequence = 0;
        while (true)
        {
            var page = await client.GetRunDetailAsync(
                runId, PageLimit, offset, cancellationToken).ConfigureAwait(false);
            if (page.Run.RunId != runId
                || offset != events.Count
                || page.SnapshotEventSequence < snapshotSequence
                || page.EventsTotal < offset + page.Events.Count)
            {
                throw Invalid($"Run Detail '{runId}' is inconsistent.");
            }
            latest = page;
            snapshotSequence = page.SnapshotEventSequence;
            foreach (var item in page.Events)
            {
                if (!eventIds.Add(item.EventId))
                {
                    throw Invalid($"Run Detail '{runId}' contains duplicate events.");
                }
                events.Add(item);
            }
            var next = checked(offset + (uint)page.Events.Count);
            if (!page.EventsHasMore)
            {
                if (next != page.EventsTotal || latest is null)
                {
                    throw Invalid($"Run Detail '{runId}' ended at an invalid offset.");
                }
                return latest with { Events = events, EventsHasMore = false };
            }
            if (next == offset) throw Invalid($"Run Detail '{runId}' did not advance.");
            offset = next;
        }
    }

    private static void ValidateAuthority(
        string accountId,
        IReadOnlyList<LocalAgentRunSnapshot> runs,
        IReadOnlyList<LocalAgentTaskSnapshot> tasks)
    {
        var runsById = runs.ToDictionary(run => run.RunId, StringComparer.Ordinal);
        foreach (var run in runs)
        {
            if (run.OwnerUserId != accountId)
            {
                throw Invalid($"Run '{run.RunId}' belongs to another account.");
            }
        }
        foreach (var task in tasks)
        {
            if (task.RunIds.Count == 0
                || task.RunIds[0] != task.InitialRunId
                || !task.RunIds.Contains(task.CurrentRunId, StringComparer.Ordinal)
                || task.RunIds.Distinct(StringComparer.Ordinal).Count() != task.RunIds.Count)
            {
                throw Invalid($"Task '{task.TaskId}' has invalid Run history.");
            }
            foreach (var runId in task.RunIds)
            {
                if (!runsById.TryGetValue(runId, out var run)
                    || run.ProfileKey != "task_runner"
                    || run.OwnerEntityType != "task"
                    || run.OwnerEntityId != task.TaskId
                    || run.ProjectId != task.ProjectId)
                {
                    throw Invalid($"Task '{task.TaskId}' does not own Run '{runId}'.");
                }
            }
        }
    }

    private static void ValidateRunDetail(
        LocalAgentRunSnapshot listed,
        LocalAgentRunSnapshot detailed)
    {
        if (detailed.RunId != listed.RunId
            || detailed.OwnerUserId != listed.OwnerUserId
            || detailed.ProfileKey != listed.ProfileKey
            || detailed.OwnerEntityType != listed.OwnerEntityType
            || detailed.OwnerEntityId != listed.OwnerEntityId
            || detailed.ProjectId != listed.ProjectId
            || detailed.Version < listed.Version)
        {
            throw Invalid($"Run Detail '{listed.RunId}' changed frozen identity.");
        }
    }

    internal static void ValidateBinding(
        LocalAgentRunSnapshot run,
        LocalAgentMainChatRunBinding binding)
    {
        var message = binding.UserMessage;
        if (binding.RunId != run.RunId
            || binding.ThreadId != run.OwnerEntityId
            || string.IsNullOrWhiteSpace(binding.TurnId)
            || string.IsNullOrWhiteSpace(binding.MessageId)
            || message.RecordId != binding.MessageId
            || message.RunId != binding.RunId
            || message.ThreadId != binding.ThreadId
            || message.TurnId != binding.TurnId
            || message.Role != LocalAgentStoredMessageRole.User
            || message.MessageMode != LocalAgentStoredMessageMode.Semantic
            || message.MessageSource != "main_chat")
        {
            throw Invalid($"Main Chat binding for Run '{run.RunId}' is inconsistent.");
        }
    }

    private static void ValidateCursor(string? current, string? next, string kind)
    {
        if (next is not null && next == current)
        {
            throw Invalid($"{kind} pagination did not advance.");
        }
    }

    private static InvalidDataException Invalid(string message) => new(message);
}
