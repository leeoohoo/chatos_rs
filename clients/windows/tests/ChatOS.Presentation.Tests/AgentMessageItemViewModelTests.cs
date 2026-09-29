using ChatOS.Core.Domain;
using ChatOS.Presentation.AgentTeams;

namespace ChatOS.Presentation.Tests;

public sealed class AgentMessageItemViewModelTests
{
    [Fact]
    public void UsesFriendlySenderLabelsAndAlignmentKinds()
    {
        var human = new AgentMessageItemViewModel(Message(AgentMessageSenderKind.Human));
        var agent = new AgentMessageItemViewModel(
            Message(AgentMessageSenderKind.Agent, "agent-1"), "Windows 专家");
        var system = new AgentMessageItemViewModel(Message(AgentMessageSenderKind.System));

        Assert.Equal("你", human.SenderLabel);
        Assert.True(human.IsHuman);
        Assert.Equal("Windows 专家", agent.SenderLabel);
        Assert.False(agent.IsHuman);
        Assert.Equal("系统", system.SenderLabel);
        Assert.Equal("\uE946", system.SenderGlyph);
    }

    [Fact]
    public void FormatsTimestampInsteadOfExposingUnixMilliseconds()
    {
        var createdAt = DateTimeOffset.Now.AddYears(-2);
        var item = new AgentMessageItemViewModel(
            Message(AgentMessageSenderKind.Human, createdAtUnixMs: createdAt.ToUnixTimeMilliseconds()));

        Assert.Contains(createdAt.Year.ToString(), item.TimestampLabel);
        Assert.Contains(":", item.TimestampLabel);
        Assert.DoesNotContain(createdAt.ToUnixTimeMilliseconds().ToString(), item.TimestampLabel);
    }

    private static AgentMessage Message(
        AgentMessageSenderKind kind,
        string? agentId = null,
        long? createdAtUnixMs = null) =>
        new(
            "message-1",
            "owner-1",
            "room-1",
            kind,
            agentId,
            "hello",
            [],
            [],
            null,
            "message-1",
            0,
            createdAtUnixMs ?? DateTimeOffset.Now.ToUnixTimeMilliseconds());
}
