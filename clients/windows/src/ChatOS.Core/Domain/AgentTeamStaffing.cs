namespace ChatOS.Core.Domain;

public static class AgentProfilePermissions
{
    public const string StaffHire = "agent.staff.hire";
    public const string StaffTerminate = "agent.staff.terminate";
    public const string LegacyProjectSteward = "builtin.project-steward";

    public static bool CanManageStaff(AgentProfile profile) =>
        CanManageStaff(profile.Draft.Skills);

    public static bool CanManageStaff(IReadOnlyList<string> permissions)
    {
        var values = permissions.ToHashSet(StringComparer.Ordinal);
        return values.Contains(LegacyProjectSteward) ||
            values.Contains(StaffHire) && values.Contains(StaffTerminate);
    }

    public static IReadOnlyList<string> NormalizeStaffPermissions(
        IReadOnlyList<string> permissions,
        bool canManageStaff)
    {
        var values = permissions.ToHashSet(StringComparer.Ordinal);
        values.Remove(LegacyProjectSteward);
        values.Remove(StaffHire);
        values.Remove(StaffTerminate);
        if (canManageStaff)
        {
            values.Add(StaffHire);
            values.Add(StaffTerminate);
        }
        return values.Order(StringComparer.Ordinal).ToArray();
    }
}

public enum AgentStaffingProposalKind
{
    CreateAgent,
    AddExistingAgent,
    RemoveMember,
}

public enum AgentStaffingProposalStatus
{
    Pending,
    Approved,
    Rejected,
}

public sealed record AgentStaffingProposalDraft(
    AgentStaffingProposalKind Kind,
    string Name = "",
    string Role = "",
    string Responsibility = "",
    string RolePrompt = "",
    string? ThinkingLevel = null,
    string ProfessionKey = "general",
    string Rationale = "",
    string? TargetRoomId = null,
    string? TargetAgentId = null,
    string Reason = "",
    string HandoffPlan = "")
{
    public void Validate()
    {
        switch (Kind)
        {
            case AgentStaffingProposalKind.CreateAgent:
                AgentTeamValidation.Text(Name, nameof(Name), 120);
                AgentTeamValidation.Text(Role, nameof(Role), 160);
                AgentTeamValidation.OptionalText(Responsibility, nameof(Responsibility), 8_000);
                AgentTeamValidation.Text(RolePrompt, nameof(RolePrompt), 32_000);
                AgentTeamValidation.OptionalText(ThinkingLevel, nameof(ThinkingLevel), 32);
                AgentTeamValidation.Identifier(ProfessionKey, nameof(ProfessionKey));
                AgentTeamValidation.OptionalText(Rationale, nameof(Rationale), 4_000);
                break;
            case AgentStaffingProposalKind.AddExistingAgent:
                AgentTeamValidation.Identifier(TargetRoomId, nameof(TargetRoomId));
                AgentTeamValidation.Identifier(TargetAgentId, nameof(TargetAgentId));
                AgentTeamValidation.Text(Role, nameof(Role), 160);
                AgentTeamValidation.OptionalText(Responsibility, nameof(Responsibility), 8_000);
                break;
            case AgentStaffingProposalKind.RemoveMember:
                AgentTeamValidation.Identifier(TargetAgentId, nameof(TargetAgentId));
                AgentTeamValidation.Text(Reason, nameof(Reason), 4_000);
                AgentTeamValidation.OptionalText(HandoffPlan, nameof(HandoffPlan), 8_000);
                break;
            default:
                throw AgentTeamValidation.Invalid(nameof(Kind));
        }
    }
}

public sealed record AgentStaffingProposal(
    string Id,
    string OwnerUserId,
    string SourceRoomId,
    string ProposerAgentId,
    string SourceDeliveryId,
    string RequestKey,
    AgentStaffingProposalDraft Draft,
    AgentStaffingProposalStatus Status,
    string? CreatedAgentId,
    long CreatedAtUnixMs,
    long? ResolvedAtUnixMs)
{
    public void Validate()
    {
        AgentTeamValidation.Identifier(Id, nameof(Id));
        AgentTeamValidation.Identifier(OwnerUserId, nameof(OwnerUserId));
        AgentTeamValidation.Identifier(SourceRoomId, nameof(SourceRoomId));
        AgentTeamValidation.Identifier(ProposerAgentId, nameof(ProposerAgentId));
        AgentTeamValidation.Identifier(SourceDeliveryId, nameof(SourceDeliveryId));
        AgentTeamValidation.Identifier(RequestKey, nameof(RequestKey));
        Draft.Validate();
        if (CreatedAgentId is not null)
            AgentTeamValidation.Identifier(CreatedAgentId, nameof(CreatedAgentId));
        if (CreatedAtUnixMs < 0 ||
            ResolvedAtUnixMs is { } resolvedAt && resolvedAt < CreatedAtUnixMs)
            throw AgentTeamValidation.Invalid("proposal timestamps");
    }
}
