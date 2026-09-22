using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed partial class AgentTeamToolExecutor
{
    public static IReadOnlyList<AgentToolDefinition> StaffingDefinitions { get; } =
    [
        Tool("agent_propose_member",
            "向 Human 提议创建一个可复用 Agent；审批前不会创建，团队会话批准后自动入队。", new
            {
                type = "object",
                properties = new
                {
                    name = new { type = "string", maxLength = 120 },
                    role = new { type = "string", maxLength = 160 },
                    responsibility = new { type = "string", maxLength = 8_000 },
                    role_prompt = new { type = "string", maxLength = 32_000 },
                    thinking_level = new { type = "string", maxLength = 32 },
                    profession_key = new { type = "string", maxLength = 512 },
                    rationale = new { type = "string", maxLength = 4_000 },
                },
                required = new[] { "name", "role", "role_prompt", "profession_key" },
                additionalProperties = false,
            }),
        Tool("agent_propose_existing_member",
            "向 Human 提议把账户中已有的 Agent 加入指定项目团队；审批前不会改变成员关系。", new
            {
                type = "object",
                properties = new
                {
                    target_room_id = new { type = "string", maxLength = 512 },
                    target_agent_id = new { type = "string", maxLength = 512 },
                    role = new { type = "string", maxLength = 160 },
                    responsibility = new { type = "string", maxLength = 8_000 },
                },
                required = new[] { "target_room_id", "target_agent_id", "role" },
                additionalProperties = false,
            }),
        Tool("agent_propose_member_removal",
            "向 Human 提议把成员移出当前团队；Agent Profile 保留，项目经理必须先完成交接。", new
            {
                type = "object",
                properties = new
                {
                    target_agent_id = new { type = "string", maxLength = 512 },
                    reason = new { type = "string", maxLength = 4_000 },
                    handoff_plan = new { type = "string", maxLength = 8_000 },
                },
                required = new[] { "target_agent_id", "reason" },
                additionalProperties = false,
            }),
    ];

    private async Task<AgentToolExecutionResult> ProposeMemberAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentDelivery delivery,
        string requestKey,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        RequireStaffingCapability(profile);
        var proposal = await store.CreateStaffingProposalAsync(profile.OwnerUserId, room.Id,
            profile.Id, delivery.Id, requestKey,
            new AgentStaffingProposalDraft(AgentStaffingProposalKind.CreateAgent,
                Name: RequiredString(arguments, "name"),
                Role: RequiredString(arguments, "role"),
                Responsibility: OptionalString(arguments, "responsibility") ?? string.Empty,
                RolePrompt: RequiredString(arguments, "role_prompt"),
                ThinkingLevel: OptionalString(arguments, "thinking_level"),
                ProfessionKey: RequiredString(arguments, "profession_key"),
                Rationale: OptionalString(arguments, "rationale") ?? string.Empty),
            cancellationToken).ConfigureAwait(false);
        return ProposalAcknowledgement(proposal);
    }

    private async Task<AgentToolExecutionResult> ProposeExistingMemberAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentDelivery delivery,
        string requestKey,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        RequireStaffingCapability(profile);
        var proposal = await store.CreateStaffingProposalAsync(profile.OwnerUserId, room.Id,
            profile.Id, delivery.Id, requestKey,
            new AgentStaffingProposalDraft(AgentStaffingProposalKind.AddExistingAgent,
                Role: RequiredString(arguments, "role"),
                Responsibility: OptionalString(arguments, "responsibility") ?? string.Empty,
                TargetRoomId: RequiredString(arguments, "target_room_id"),
                TargetAgentId: RequiredString(arguments, "target_agent_id")),
            cancellationToken).ConfigureAwait(false);
        return ProposalAcknowledgement(proposal);
    }

    private async Task<AgentToolExecutionResult> ProposeMemberRemovalAsync(
        AgentProfile profile,
        AgentRoom room,
        AgentDelivery delivery,
        string requestKey,
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        RequireStaffingCapability(profile);
        var proposal = await store.CreateStaffingProposalAsync(profile.OwnerUserId, room.Id,
            profile.Id, delivery.Id, requestKey,
            new AgentStaffingProposalDraft(AgentStaffingProposalKind.RemoveMember,
                TargetAgentId: RequiredString(arguments, "target_agent_id"),
                Reason: RequiredString(arguments, "reason"),
                HandoffPlan: OptionalString(arguments, "handoff_plan") ?? string.Empty),
            cancellationToken).ConfigureAwait(false);
        return ProposalAcknowledgement(proposal);
    }

    private static AgentToolExecutionResult ProposalAcknowledgement(
        AgentStaffingProposal proposal) => new(Json(new
        {
            proposal_id = proposal.Id,
            type = proposal.Draft.Kind.ToString(),
            status = proposal.Status.ToString(),
            awaiting_human_approval = proposal.Status == AgentStaffingProposalStatus.Pending,
        }), EndsCycle: true);

    private static void RequireStaffingCapability(AgentProfile profile)
    {
        if (!AgentProfilePermissions.CanManageStaff(profile))
            throw new AgentTeamException(AgentTeamError.PermissionDenied,
                "The Agent is not allowed to manage team members.");
    }
}
