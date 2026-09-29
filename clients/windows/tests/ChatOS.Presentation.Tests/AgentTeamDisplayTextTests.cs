using ChatOS.Core.Domain;
using ChatOS.Presentation.AgentTeams;

namespace ChatOS.Presentation.Tests;

public sealed class AgentTeamDisplayTextTests
{
    [Theory]
    [InlineData(AgentTodoStatus.InProgress, "进行中")]
    [InlineData(AgentRunStatus.Failed, "失败")]
    [InlineData(AgentTeamAssetCategory.Deliverable, "交付物")]
    [InlineData(AgentStaffingProposalKind.RemoveMember, "移除成员")]
    [InlineData(AgentConversationKind.HumanAgentDirect, "Agent 私聊")]
    public void PresentsDomainEnumsAsUserFacingText(object value, string expected)
    {
        Assert.Equal(expected, AgentTeamDisplayText.For(value));
    }
}
