using ChatOS.Core.Domain;

namespace ChatOS.Presentation.AgentTeams;

public static class AgentTeamDisplayText
{
    public static string For(object? value) => value switch
    {
        AgentConversationKind.ProjectTeam => "项目团队",
        AgentConversationKind.HumanAgentDirect => "Agent 私聊",
        AgentConversationKind.AgentAgentDirect => "Agent 间私聊",
        AgentTodoStatus.Pending => "待处理",
        AgentTodoStatus.Ready => "可开始",
        AgentTodoStatus.InProgress => "进行中",
        AgentTodoStatus.Blocked => "已阻塞",
        AgentTodoStatus.Completed => "已完成",
        AgentTodoStatus.Cancelled => "已取消",
        AgentTodoPriority.Low => "低",
        AgentTodoPriority.Normal => "普通",
        AgentTodoPriority.High => "高",
        AgentTodoPriority.Urgent => "紧急",
        AgentTeamAssetCategory.Overview => "项目概览",
        AgentTeamAssetCategory.CurrentProgress => "当前进展",
        AgentTeamAssetCategory.TechStack => "技术栈",
        AgentTeamAssetCategory.Architecture => "架构",
        AgentTeamAssetCategory.Conventions => "规范",
        AgentTeamAssetCategory.Requirement => "需求",
        AgentTeamAssetCategory.Plan => "计划",
        AgentTeamAssetCategory.Decision => "决策",
        AgentTeamAssetCategory.Research => "调研",
        AgentTeamAssetCategory.Deliverable => "交付物",
        AgentTeamAssetCategory.Note => "笔记",
        AgentTeamAssetCategory.Reference => "参考资料",
        AgentStaffingProposalKind.CreateAgent => "新建 Agent",
        AgentStaffingProposalKind.AddExistingAgent => "加入现有 Agent",
        AgentStaffingProposalKind.RemoveMember => "移除成员",
        AgentStaffingProposalStatus.Pending => "待审批",
        AgentStaffingProposalStatus.Approved => "已批准",
        AgentStaffingProposalStatus.Rejected => "已拒绝",
        AgentRunStatus.Pending => "等待中",
        AgentRunStatus.Running => "运行中",
        AgentRunStatus.Paused => "已暂停",
        AgentRunStatus.Completed => "已完成",
        AgentRunStatus.Failed => "失败",
        AgentRunStatus.Cancelled => "已取消",
        null => string.Empty,
        _ => value.ToString() ?? string.Empty,
    };
}

public sealed record AgentEnumOption<T>(T Value, string Label) where T : struct, Enum;
