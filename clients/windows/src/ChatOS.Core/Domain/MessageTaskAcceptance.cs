using System.Text;
using System.Text.Json;

namespace ChatOS.Core.Domain;

/// <summary>
/// Presentation-only interpretation of a Task result. This must never be used to
/// change scheduling, dependency gates, or the persisted Task status.
/// </summary>
public enum MessageTaskResultDisposition
{
    ExecutionFailed,
    Blocked,
    EvidenceIncomplete,
    AcceptanceFailed,
    AcceptancePassed,
}

public sealed record MessageTaskAcceptanceContract(
    int Version,
    IReadOnlyList<string> Criteria,
    int? FinalResponseMaxChars)
{
    public int EffectiveFinalResponseMaxChars => FinalResponseMaxChars ?? 32_000;

    public static MessageTaskAcceptanceContract? Decode(JsonElement input)
    {
        if (input.ValueKind != JsonValueKind.Object ||
            !input.TryGetProperty("acceptance_report_version", out var versionValue) ||
            !versionValue.TryGetInt32(out var version) || version != 1 ||
            !input.TryGetProperty("acceptance_criteria", out var criteriaValue) ||
            criteriaValue.ValueKind != JsonValueKind.Array)
        {
            return null;
        }

        var criteria = new List<string>();
        foreach (var value in criteriaValue.EnumerateArray())
        {
            if (value.ValueKind != JsonValueKind.String ||
                string.IsNullOrWhiteSpace(value.GetString()))
            {
                return null;
            }
            criteria.Add(value.GetString()!);
        }
        if (criteria.Count == 0) return null;

        int? maximum = null;
        if (input.TryGetProperty("final_response_max_chars", out var maximumValue) &&
            maximumValue.ValueKind != JsonValueKind.Null)
        {
            if (!maximumValue.TryGetInt32(out var parsed) || parsed is < 1 or > 32_000)
                return null;
            maximum = parsed;
        }

        return new MessageTaskAcceptanceContract(version, criteria, maximum);
    }
}

public sealed record MessageTaskAcceptanceCheck(
    int CriterionIndex,
    string Criterion,
    string Status,
    string Evidence)
{
    public int DisplayIndex => CriterionIndex + 1;
}

public sealed record MessageTaskAcceptanceReport(
    string Status,
    string Reason,
    string FinalResponse,
    int FinalResponseChars,
    IReadOnlyList<MessageTaskAcceptanceCheck> Checks)
{
    public MessageTaskResultDisposition Disposition => Status switch
    {
        "blocked" => MessageTaskResultDisposition.Blocked,
        "succeeded" => MessageTaskResultDisposition.AcceptancePassed,
        _ when Checks.Any(static value => value.Status == "failed") =>
            MessageTaskResultDisposition.AcceptanceFailed,
        _ when Checks.Any(static value => value.Status == "not_verified") =>
            MessageTaskResultDisposition.EvidenceIncomplete,
        _ => MessageTaskResultDisposition.AcceptanceFailed,
    };

    public static MessageTaskAcceptanceReport? Decode(
        JsonElement? terminalOutcome,
        MessageTaskAcceptanceContract? contract)
    {
        if (contract is null || terminalOutcome is not { ValueKind: JsonValueKind.Object } outcome ||
            !outcome.TryGetProperty("task_acceptance", out var report) ||
            report.ValueKind != JsonValueKind.Object ||
            !TryString(report, "status", out var status) ||
            status is not ("succeeded" or "failed" or "blocked") ||
            !TryString(report, "final_response", out var finalResponse) ||
            string.IsNullOrWhiteSpace(finalResponse) ||
            !report.TryGetProperty("acceptance_checks", out var checksValue) ||
            checksValue.ValueKind != JsonValueKind.Array ||
            checksValue.GetArrayLength() != contract.Criteria.Count)
        {
            return null;
        }

        var finalResponseChars = CountUnicodeScalars(finalResponse);
        if (finalResponseChars > contract.EffectiveFinalResponseMaxChars) return null;

        var seen = new bool[contract.Criteria.Count];
        var checks = new List<MessageTaskAcceptanceCheck>(contract.Criteria.Count);
        foreach (var check in checksValue.EnumerateArray())
        {
            if (check.ValueKind != JsonValueKind.Object ||
                !check.TryGetProperty("criterion_index", out var indexValue) ||
                !indexValue.TryGetInt32(out var index) ||
                index < 0 || index >= contract.Criteria.Count || seen[index] ||
                !TryString(check, "status", out var checkStatus) ||
                checkStatus is not ("passed" or "failed" or "not_verified") ||
                !TryString(check, "evidence", out var evidence) ||
                string.IsNullOrWhiteSpace(evidence) || CountUnicodeScalars(evidence) > 2_000 ||
                status == "succeeded" && checkStatus != "passed")
            {
                return null;
            }

            seen[index] = true;
            checks.Add(new MessageTaskAcceptanceCheck(
                index, contract.Criteria[index], checkStatus, evidence));
        }

        if (seen.Any(static value => !value)) return null;
        checks.Sort(static (left, right) => left.CriterionIndex.CompareTo(right.CriterionIndex));
        var reason = TryString(report, "reason", out var parsedReason) ? parsedReason : string.Empty;
        return new MessageTaskAcceptanceReport(
            status, reason, finalResponse, finalResponseChars, checks);
    }

    private static int CountUnicodeScalars(string value) => value.EnumerateRunes().Count();

    private static bool TryString(JsonElement value, string name, out string result)
    {
        result = string.Empty;
        if (!value.TryGetProperty(name, out var child) || child.ValueKind != JsonValueKind.String)
            return false;
        result = child.GetString() ?? string.Empty;
        return true;
    }
}
