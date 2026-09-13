using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

internal static class WindowsLocalAgentRunSnapshotComparer
{
    public static bool Same(LocalAgentRunSnapshot left, LocalAgentRunSnapshot right) =>
        left.RunId == right.RunId
        && left.ProfileKey == right.ProfileKey
        && left.OwnerUserId == right.OwnerUserId
        && left.OwnerEntityType == right.OwnerEntityType
        && left.OwnerEntityId == right.OwnerEntityId
        && left.ProjectId == right.ProjectId
        && left.Status == right.Status
        && left.Version == right.Version
        && left.StepSeq == right.StepSeq
        && left.Iteration == right.Iteration
        && left.RetryCount == right.RetryCount
        && left.ModelConfigId == right.ModelConfigId
        && left.ModelConfigRevision == right.ModelConfigRevision
        && Json(left.ModelRuntimeSnapshot) == Json(right.ModelRuntimeSnapshot)
        && left.ContextStrategy == right.ContextStrategy
        && left.PromptRevision == right.PromptRevision
        && left.CapabilitySnapshotRef == right.CapabilitySnapshotRef
        && left.PendingBatchId == right.PendingBatchId
        && Json(left.PendingInteraction) == Json(right.PendingInteraction)
        && Json(left.TerminalOutcome) == Json(right.TerminalOutcome)
        && left.DeadlineAt == right.DeadlineAt
        && left.CreatedAt == right.CreatedAt
        && left.UpdatedAt == right.UpdatedAt;

    private static string? Json(JsonElement? value) =>
        value is { } element ? element.GetRawText() : null;

    private static string Json(JsonElement value) => value.GetRawText();
}
