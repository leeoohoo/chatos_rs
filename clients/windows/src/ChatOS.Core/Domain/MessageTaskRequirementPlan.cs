using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Core.Domain;

/// <summary>
/// Presentation of the immutable requirement-plan snapshot stored in this
/// Task's own input. Callers must not replace it with the project's current
/// plan when rendering an older Task or retry.
/// </summary>
public sealed record MessageTaskRequirementPlanSnapshot(
    string PlanId,
    ulong Revision,
    string Title,
    string Markdown,
    long ConfirmedAtUnixMs);

public sealed record MessageTaskRequirementPlanScope(
    string Title,
    string Prompt,
    string ExpectedOutputs,
    string AcceptanceCriteria);

public sealed record MessageTaskRequirementPlan(
    MessageTaskRequirementPlanSnapshot Snapshot,
    string MarkdownSha256,
    MessageTaskRequirementPlanScope TaskScope)
{
    public static MessageTaskRequirementPlan? Decode(JsonElement input, string? taskTitle = null)
    {
        if (input.ValueKind != JsonValueKind.Object ||
            !input.TryGetProperty("requirement_plan", out var plan) ||
            plan.ValueKind != JsonValueKind.Object ||
            !plan.TryGetProperty("snapshot", out var snapshot) ||
            snapshot.ValueKind != JsonValueKind.Object ||
            !TryRequiredString(snapshot, "plan_id", out var planId) ||
            !snapshot.TryGetProperty("revision", out var revisionValue) ||
            !revisionValue.TryGetUInt64(out var revision) || revision == 0 ||
            !TryRequiredString(snapshot, "title", out var title) ||
            !TryRequiredString(snapshot, "markdown", out var markdown) ||
            markdown.Contains('\0') ||
            !snapshot.TryGetProperty("confirmed_at_unix_ms", out var confirmedValue) ||
            !confirmedValue.TryGetInt64(out var confirmedAtUnixMs) || confirmedAtUnixMs <= 0 ||
            !TryRequiredString(plan, "markdown_sha256", out var hash) ||
            hash.Length != 64 || !hash.All(Uri.IsHexDigit) ||
            !plan.TryGetProperty("task_scope", out var scope) ||
            scope.ValueKind != JsonValueKind.Object ||
            !TryRequiredString(scope, "title", out var scopeTitle) ||
            (!string.IsNullOrWhiteSpace(taskTitle) &&
             !string.Equals(scopeTitle, taskTitle, StringComparison.Ordinal)))
        {
            return null;
        }

        var calculatedHash = Convert.ToHexString(
            SHA256.HashData(Encoding.UTF8.GetBytes(markdown))).ToLowerInvariant();
        if (!string.Equals(hash, calculatedHash, StringComparison.OrdinalIgnoreCase)) return null;

        return new MessageTaskRequirementPlan(
            new MessageTaskRequirementPlanSnapshot(
                planId, revision, title, markdown, confirmedAtUnixMs),
            hash.ToLowerInvariant(),
            new MessageTaskRequirementPlanScope(
                scopeTitle,
                DisplayValue(scope, "prompt"),
                DisplayValue(scope, "expected_outputs"),
                DisplayValue(scope, "acceptance_criteria")));
    }

    private static bool TryRequiredString(JsonElement value, string name, out string result)
    {
        result = string.Empty;
        if (!value.TryGetProperty(name, out var child) || child.ValueKind != JsonValueKind.String)
            return false;
        result = child.GetString()?.Trim() ?? string.Empty;
        return result.Length > 0;
    }

    private static string DisplayValue(JsonElement value, string name)
    {
        if (!value.TryGetProperty(name, out var child) || child.ValueKind == JsonValueKind.Null)
            return string.Empty;
        if (child.ValueKind == JsonValueKind.String) return child.GetString()?.Trim() ?? string.Empty;
        if (child.ValueKind == JsonValueKind.Array)
        {
            var items = child.EnumerateArray().Select(item => item.ValueKind == JsonValueKind.String
                ? item.GetString()?.Trim()
                : item.GetRawText()).Where(item => !string.IsNullOrWhiteSpace(item)).ToArray();
            return string.Join(Environment.NewLine, items.Select(item => $"• {item}"));
        }
        return child.GetRawText();
    }
}
