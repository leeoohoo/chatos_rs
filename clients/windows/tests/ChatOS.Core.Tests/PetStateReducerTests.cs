using ChatOS.Core.Domain;
using ChatOS.Core.State;

namespace ChatOS.Core.Tests;

public sealed class PetStateReducerTests
{
    [Fact]
    public void ApprovalOutranksRunningWorkAndCountsBoth()
    {
        var reducer = new PetStateReducer();
        reducer.Apply(new PetActivityEvent.Upsert(new PetActivity(
            "chat:turn-1",
            PetActivitySource.Chat,
            PetActivityKind.Working,
            "AI 正在执行")));
        reducer.Apply(new PetActivityEvent.Upsert(new PetActivity(
            "approval:1",
            PetActivitySource.LocalApproval,
            PetActivityKind.WaitingForApproval,
            "等待审批")));

        var presentation = reducer.Presentation();

        Assert.Equal("approval:1", presentation.PrimaryActivity?.Id);
        Assert.Equal(PetAnimationState.Waiting, presentation.AnimationState);
        Assert.Equal(1, presentation.ActiveWorkCount);
        Assert.Equal(1, presentation.AttentionCount);
    }

    [Fact]
    public void DuplicateEventDoesNotOverwriteNewerActivity()
    {
        var reducer = new PetStateReducer();
        reducer.Apply(new PetActivityEvent.Upsert(new PetActivity(
            "task:1",
            PetActivitySource.TaskRunner,
            PetActivityKind.Succeeded,
            "已完成",
            eventId: "event-2",
            eventSequence: 2)));
        reducer.Apply(new PetActivityEvent.Upsert(new PetActivity(
            "task:1",
            PetActivitySource.TaskRunner,
            PetActivityKind.Working,
            "运行中",
            eventId: "event-1",
            eventSequence: 1)));

        Assert.Equal(PetActivityKind.Succeeded, reducer.Presentation().PrimaryActivity?.Kind);
    }

    [Fact]
    public void ExpiredTransientActivityReturnsToIdle()
    {
        var now = DateTimeOffset.Parse("2026-08-30T00:00:00Z");
        var reducer = new PetStateReducer();
        reducer.Apply(new PetActivityEvent.Upsert(new PetActivity(
            "approval-result:1",
            PetActivitySource.LocalApproval,
            PetActivityKind.Succeeded,
            "已允许",
            expiresAt: now.AddSeconds(5))), now);

        Assert.NotNull(reducer.Presentation(now).PrimaryActivity);
        Assert.Null(reducer.Presentation(now.AddSeconds(6)).PrimaryActivity);
    }

    [Fact]
    public void StableIdentityChangesWhenActivityVersionChanges()
    {
        var first = new PetActivity(
            "task:1",
            PetActivitySource.TaskRunner,
            PetActivityKind.Blocked,
            "阻塞",
            activityVersion: "run-1");
        var second = first with { ActivityVersion = "run-2" };

        Assert.NotEqual(first.StableIdentity, second.StableIdentity);
    }
}
