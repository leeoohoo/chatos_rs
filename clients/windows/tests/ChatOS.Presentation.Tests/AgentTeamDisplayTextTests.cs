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

    [Fact]
    public void TodoAndRunItemsResolveAgentNamesAndLabels()
    {
        var now = DateTimeOffset.Now.ToUnixTimeMilliseconds();
        var todo = new AgentTodo("todo-1", "owner-1",
            new AgentTodoDraft("room-1", "agent-1", "修复打包", Priority: AgentTodoPriority.Urgent),
            AgentTodoStatus.InProgress, "", 0, 1, now, now);
        var run = new AgentRunSummary("run-1", "owner-1", "delivery-1", "agent-1", "room-1",
            AgentRunStatus.Running, 3, null, now, now);

        var todoItem = new AgentTodoItemViewModel(todo, "Windows 专家");
        var runItem = new AgentRunItemViewModel(run, "Windows 专家");

        Assert.Equal("Windows 专家", todoItem.AssigneeLabel);
        Assert.Equal("紧急优先级", todoItem.PriorityLabel);
        Assert.Equal("进行中", todoItem.StatusLabel);
        Assert.Equal("Windows 专家", runItem.AgentLabel);
        Assert.Equal("模型调用 3 次", runItem.ModelCallsLabel);
    }
}
