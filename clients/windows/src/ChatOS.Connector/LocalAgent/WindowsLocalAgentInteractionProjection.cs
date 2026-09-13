using System.Text.Json;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalAgentProjectionRoute(
    WindowsLocalAgentRecoveredRun Recovered,
    WindowsLocalAgentRunSource Source)
{
    public LocalAgentRunSnapshot Run => Recovered.Run;
}

/// <summary>
/// The single pure mapping from an immutable account projection to Windows
/// interaction models. Main Chat, Task, Pet, Ask User, tool approval and Run
/// controls all consume this mapper so none of them infer owner identity from UI state.
/// </summary>
internal static class WindowsLocalAgentInteractionProjection
{
    public static IReadOnlyList<WindowsLocalAgentProjectionRoute> Routes(
        WindowsLocalAgentProjectionSnapshot projection)
    {
        var routes = projection.Runs.Values.Select(recovered =>
        {
            var source = WindowsLocalAgentRunSourceResolver.Resolve(projection, recovered);
            return source is null ? null : new WindowsLocalAgentProjectionRoute(recovered, source);
        }).Where(route => route is not null).Cast<WindowsLocalAgentProjectionRoute>().ToArray();
        if (routes.Select(route => route.Run.RunId).Distinct(StringComparer.Ordinal).Count()
            != routes.Length)
            throw new InvalidDataException("The Local Agent projection contains duplicate Run identities.");
        return routes;
    }

    public static LocalAgentRunControlState RunControl(WindowsLocalAgentProjectionRoute route)
    {
        var run = route.Run;
        return new LocalAgentRunControlState(
            run.RunId, run.Version, route.Source.ThreadId, route.Source.TurnId, run.Status,
            run.Iteration, run.RetryCount, InteractionKind(run.PendingInteraction),
            ReviewReason(run.PendingInteraction), run.UpdatedAt);
    }

    public static IReadOnlyList<LocalAgentToolApprovalRequest> ToolApprovals(
        WindowsLocalAgentProjectionRoute route)
    {
        var recovered = route.Recovered;
        if (recovered.Run.Status is LocalAgentRunStatus.Succeeded
            or LocalAgentRunStatus.Failed or LocalAgentRunStatus.Cancelled) return [];
        var detail = recovered.Detail
            ?? throw new InvalidDataException("The Local Agent run has no authoritative detail.");
        if (!WindowsLocalAgentRunSnapshotComparer.Same(recovered.Run, detail.Run))
            throw new InvalidDataException(
                "The Local Agent run and tool detail projections are inconsistent.");
        var approvals = detail.Tools.Where(tool =>
                tool.Status == LocalAgentToolExecutionStatus.AwaitingApproval)
            .Select(tool =>
            {
                if (!string.Equals(tool.RunId, recovered.Run.RunId, StringComparison.Ordinal))
                    throw new InvalidDataException(
                        "The Local Agent tool invocation changed its owning run.");
                return new LocalAgentToolApprovalRequest(
                    tool.InvocationId, recovered.Run.RunId, route.Source.ThreadId,
                    route.Source.TurnId, tool.ToolName, tool.Effect, tool.ArgumentsDigest);
            }).ToArray();
        if (approvals.Select(value => value.InvocationId).Distinct(StringComparer.Ordinal).Count()
            != approvals.Length)
            throw new InvalidDataException(
                "The Local Agent Run contains duplicate tool invocation identities.");
        return approvals;
    }

    public static bool TryAskUserPrompt(
        WindowsLocalAgentProjectionRoute route,
        out AskUserPrompt prompt)
    {
        var run = route.Run;
        prompt = null!;
        if (run.Status != LocalAgentRunStatus.Paused
            || run.PendingInteraction is not { ValueKind: JsonValueKind.Object } pending)
            return false;
        if (InteractionKind(run.PendingInteraction) != "ask_user") return false;
        var interactionId = RequiredString(pending, "interaction_id");
        if (!pending.TryGetProperty("question", out var question)
            || question.ValueKind != JsonValueKind.Object)
            throw new InvalidDataException("The Local Agent question payload is invalid.");
        var message = RequiredString(question, "prompt");
        var options = RequiredArray(question, "options").EnumerateArray().Select(option =>
        {
            if (option.ValueKind != JsonValueKind.Object)
                throw new InvalidDataException("The Local Agent question option is invalid.");
            return new AskUserChoiceOption(
                RequiredString(option, "option_id"),
                RequiredString(option, "label"),
                OptionalString(option, "description"));
        }).ToArray();
        if (options.Select(option => option.Value).Distinct(StringComparer.Ordinal).Count()
            != options.Length)
            throw new InvalidDataException("The Local Agent question contains duplicate options.");
        var imageReferences = RequiredArray(question, "image_references")
            .EnumerateArray().Select(value => value.ValueKind == JsonValueKind.String
                ? ValidString(value.GetString(), "image reference")
                : throw new InvalidDataException("The Local Agent image reference is invalid."))
            .ToArray();
        var details = question.TryGetProperty("details", out var detailsValue)
            && detailsValue.ValueKind == JsonValueKind.Object ? detailsValue : default;
        var title = details.ValueKind == JsonValueKind.Object
            ? OptionalString(details, "title") ?? "需要你的确认"
            : "需要你的确认";
        var kind = details.ValueKind == JsonValueKind.Object
            ? OptionalString(details, "kind") ?? "local_agent"
            : "local_agent";
        var allowsCancel = details.ValueKind != JsonValueKind.Object
            || !details.TryGetProperty("allows_cancel", out var cancelValue)
            || cancelValue.ValueKind == JsonValueKind.True;
        var allowsMultiple = details.ValueKind == JsonValueKind.Object
            && details.TryGetProperty("allows_multiple", out var multipleValue)
            && multipleValue.ValueKind == JsonValueKind.True;
        prompt = new AskUserPrompt(
            interactionId, route.Source.ThreadId, route.Source.TurnId, null, kind,
            AskUserPromptStatus.Pending, title, message, allowsCancel, null,
            options.Length == 0
                ? [new AskUserField(
                    "answer", "回复", null, "告诉 AI 你的决定或补充信息", string.Empty,
                    true, true, false)]
                : [],
            options.Length == 0 ? null : new AskUserChoice(
                allowsMultiple, options, [], 1, allowsMultiple ? options.Length : 1),
            run.UpdatedAt, run.UpdatedAt, imageReferences);
        return true;
    }

    public static string? InteractionKind(JsonElement? interaction) =>
        interaction is { ValueKind: JsonValueKind.Object } value
        && value.TryGetProperty("type", out var type)
        && type.ValueKind == JsonValueKind.String ? type.GetString() : null;

    private static string? ReviewReason(JsonElement? interaction)
    {
        if (interaction is not { ValueKind: JsonValueKind.Object } value) return null;
        return InteractionKind(interaction) switch
        {
            "review_unknown_tool_outcome" => value.TryGetProperty("batch_id", out var batch)
                && batch.ValueKind == JsonValueKind.String
                ? $"工具批次 {batch.GetString()} 的执行结果无法确认。继续前请核对外部结果，避免重复执行。"
                : "工具执行结果无法确认。继续前请核对外部结果，避免重复执行。",
            "runtime_blocked" => value.TryGetProperty("reason", out var reason)
                && reason.ValueKind == JsonValueKind.String
                && !string.IsNullOrWhiteSpace(reason.GetString())
                ? reason.GetString()!.Trim()
                : "本地 Agent 已阻塞，需要检查执行过程后再继续。",
            _ => null,
        };
    }

    private static JsonElement RequiredArray(JsonElement value, string property)
    {
        if (!value.TryGetProperty(property, out var item) || item.ValueKind != JsonValueKind.Array)
            throw new InvalidDataException($"The Local Agent question {property} is invalid.");
        return item;
    }

    private static string RequiredString(JsonElement value, string property) =>
        value.TryGetProperty(property, out var item) && item.ValueKind == JsonValueKind.String
            ? ValidString(item.GetString(), property)
            : throw new InvalidDataException($"The Local Agent question {property} is invalid.");

    private static string? OptionalString(JsonElement value, string property) =>
        value.TryGetProperty(property, out var item) && item.ValueKind == JsonValueKind.String
            ? item.GetString()?.Trim() is { Length: > 0 } result ? result : null
            : null;

    private static string ValidString(string? value, string field) =>
        !string.IsNullOrWhiteSpace(value) && value == value.Trim() && !value.Any(char.IsControl)
            ? value
            : throw new InvalidDataException($"The Local Agent question {field} is invalid.");
}
