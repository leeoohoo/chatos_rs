using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;

namespace ChatOS.Core.Tests;

public sealed class PetActivityCoordinatorTests
{
    private static readonly DateTimeOffset Now = new(2026, 8, 30, 12, 0, 0, TimeSpan.Zero);

    [Fact]
    public async Task DisposedActivityDoesNotReturnDuringReconcileOrRealtime()
    {
        var activity = Activity("run-1");
        var inbox = new StubInboxService(activity);
        var suppressions = new MemorySuppressionStore();
        var coordinator = new PetActivityCoordinator(inbox, suppressions);
        var reducer = new PetStateReducer();
        reducer.Apply(new PetActivityEvent.Upsert(activity), Now);

        await coordinator.ApplyDispositionAsync(
            reducer,
            activity,
            PetActivityDisposition.Ignored,
            Now);
        await coordinator.ReconcileAsync(reducer, Now.AddMinutes(1));
        await coordinator.ApplyRealtimeAsync(
            reducer,
            new PetActivityEvent.Upsert(activity),
            Now.AddMinutes(2));

        Assert.Empty(reducer.VisibleActivities(Now.AddMinutes(2)));
        Assert.Equal(PetActivityDisposition.Ignored, inbox.LastDisposition);
    }

    [Fact]
    public async Task NewActivityVersionIsNotSuppressedByPreviousRun()
    {
        var oldActivity = Activity("run-1");
        var newActivity = Activity("run-2");
        var inbox = new StubInboxService(newActivity);
        var suppressions = new MemorySuppressionStore();
        var coordinator = new PetActivityCoordinator(inbox, suppressions);
        var reducer = new PetStateReducer();

        await coordinator.ApplyDispositionAsync(
            reducer,
            oldActivity,
            PetActivityDisposition.Handled,
            Now);
        await coordinator.ReconcileAsync(reducer, Now.AddMinutes(1));

        Assert.Equal("run-2", Assert.Single(reducer.VisibleActivities()).ActivityVersion);
    }

    [Fact]
    public async Task FailedServerDispositionRollsBackLocalSuppressionAndVisibility()
    {
        var activity = Activity("run-1");
        var inbox = new StubInboxService(activity) { FailDisposition = true };
        var suppressions = new MemorySuppressionStore();
        var coordinator = new PetActivityCoordinator(inbox, suppressions);
        var reducer = new PetStateReducer();
        reducer.Apply(new PetActivityEvent.Upsert(activity), Now);

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            coordinator.ApplyDispositionAsync(
                reducer,
                activity,
                PetActivityDisposition.Acknowledged,
                Now));

        Assert.Single(reducer.VisibleActivities(Now));
        Assert.False(await suppressions.IsSuppressedAsync(activity.StableIdentity, Now));
    }

    private static PetActivity Activity(string version) => new(
        "task-runner:task-1",
        PetActivitySource.TaskRunner,
        PetActivityKind.Blocked,
        "任务被阻塞",
        route: new PetActivityRoute(TaskId: "task-1", RunId: version),
        inboxId: "inbox-1",
        inboxStatus: PetActivityInboxStatus.Unread,
        activityVersion: version,
        updatedAt: Now);

    private sealed class StubInboxService : IPetActivityInboxService
    {
        private readonly IReadOnlyList<PetActivity> _activities;

        public StubInboxService(params PetActivity[] activities)
        {
            _activities = activities;
        }

        public bool FailDisposition { get; init; }

        public PetActivityDisposition? LastDisposition { get; private set; }

        public Task<IReadOnlyList<PetActivity>> FetchOpenActivitiesAsync(
            int limit = 100,
            CancellationToken cancellationToken = default) => Task.FromResult(_activities);

        public Task ApplyAsync(
            PetActivityDisposition disposition,
            PetActivity activity,
            CancellationToken cancellationToken = default)
        {
            if (FailDisposition)
            {
                throw new InvalidOperationException("gateway unavailable");
            }

            LastDisposition = disposition;
            return Task.CompletedTask;
        }
    }

    private sealed class MemorySuppressionStore : IPetActivitySuppressionStore
    {
        private readonly Dictionary<string, DateTimeOffset?> _values = new(StringComparer.Ordinal);

        public Task<bool> IsSuppressedAsync(
            string stableIdentity,
            DateTimeOffset now,
            CancellationToken cancellationToken = default) => Task.FromResult(
            _values.TryGetValue(stableIdentity, out var expiry) &&
            (expiry is null || expiry > now));

        public Task SuppressAsync(
            string stableIdentity,
            PetActivityDisposition disposition,
            DateTimeOffset suppressedAt,
            DateTimeOffset? expiresAt,
            CancellationToken cancellationToken = default)
        {
            _values[stableIdentity] = expiresAt;
            return Task.CompletedTask;
        }

        public Task RemoveAsync(
            string stableIdentity,
            CancellationToken cancellationToken = default)
        {
            _values.Remove(stableIdentity);
            return Task.CompletedTask;
        }

        public Task PruneExpiredAsync(
            DateTimeOffset now,
            CancellationToken cancellationToken = default)
        {
            foreach (var key in _values.Where(pair => pair.Value is { } expiry && expiry <= now)
                         .Select(static pair => pair.Key)
                         .ToArray())
            {
                _values.Remove(key);
            }

            return Task.CompletedTask;
        }
    }
}
