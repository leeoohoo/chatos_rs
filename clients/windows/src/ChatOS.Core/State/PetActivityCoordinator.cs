using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Core.State;

public sealed class PetActivityCoordinator
{
    private static readonly PetActivitySource[] InboxSources =
    [
        PetActivitySource.AskUserPrompt,
        PetActivitySource.Chat,
        PetActivitySource.TaskBoard,
        PetActivitySource.TaskRunner,
        PetActivitySource.ProjectExecution,
    ];

    private readonly IPetActivityInboxService _inboxService;
    private readonly IPetActivitySuppressionStore _suppressionStore;
    private readonly SemaphoreSlim _gate = new(1, 1);

    public PetActivityCoordinator(
        IPetActivityInboxService inboxService,
        IPetActivitySuppressionStore suppressionStore)
    {
        _inboxService = inboxService;
        _suppressionStore = suppressionStore;
    }

    public async Task<bool> ReconcileAsync(
        PetStateReducer reducer,
        DateTimeOffset? now = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(reducer);
        var timestamp = now ?? DateTimeOffset.UtcNow;
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await _suppressionStore.PruneExpiredAsync(timestamp, cancellationToken)
                .ConfigureAwait(false);
            var activities = await _inboxService.FetchOpenActivitiesAsync(
                cancellationToken: cancellationToken).ConfigureAwait(false);
            var visible = new List<PetActivity>(activities.Count);
            foreach (var activity in activities)
            {
                if (!await _suppressionStore.IsSuppressedAsync(
                        activity.StableIdentity,
                        timestamp,
                        cancellationToken).ConfigureAwait(false))
                {
                    visible.Add(activity);
                }
            }

            var changed = false;
            foreach (var source in InboxSources)
            {
                changed |= reducer.Replace(source, visible.Where(activity => activity.Source == source));
            }

            return changed;
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task<bool> ApplyRealtimeAsync(
        PetStateReducer reducer,
        PetActivityEvent activityEvent,
        DateTimeOffset? now = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(reducer);
        var timestamp = now ?? DateTimeOffset.UtcNow;
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (activityEvent is PetActivityEvent.Upsert upsert &&
                await _suppressionStore.IsSuppressedAsync(
                    upsert.Activity.StableIdentity,
                    timestamp,
                    cancellationToken).ConfigureAwait(false))
            {
                return reducer.Apply(new PetActivityEvent.Remove(upsert.Activity.Id), timestamp);
            }

            return reducer.Apply(activityEvent, timestamp);
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task ApplyDispositionAsync(
        PetStateReducer reducer,
        PetActivity activity,
        PetActivityDisposition disposition,
        DateTimeOffset? now = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(reducer);
        var timestamp = now ?? DateTimeOffset.UtcNow;
        var suppressionExpiry = activity.ExpiresAt is { } activityExpiry && activityExpiry > timestamp
            ? activityExpiry
            : timestamp.AddDays(30);

        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await _suppressionStore.SuppressAsync(
                activity.StableIdentity,
                disposition,
                timestamp,
                suppressionExpiry,
                cancellationToken).ConfigureAwait(false);
            reducer.Apply(new PetActivityEvent.Remove(activity.Id), timestamp);
            try
            {
                await _inboxService.ApplyAsync(disposition, activity, cancellationToken)
                    .ConfigureAwait(false);
            }
            catch
            {
                await _suppressionStore.RemoveAsync(activity.StableIdentity, cancellationToken)
                    .ConfigureAwait(false);
                reducer.Apply(new PetActivityEvent.Upsert(activity), timestamp);
                throw;
            }
        }
        finally
        {
            _gate.Release();
        }
    }
}
