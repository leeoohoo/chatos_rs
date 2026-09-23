namespace ChatOS.Core.Domain;

public enum QuickSearchResultKind
{
    Suggestion,
    ChatOS,
    Application,
    File,
    Action,
}

public enum QuickSearchBuiltInAction
{
    Screenshot,
    ScreenRecording,
    ClipboardHistory,
    OpenSettings,
    OpenRuntimePermissions,
}

public enum QuickSearchActionKind
{
    OpenProject,
    OpenContact,
    OpenApplication,
    OpenFile,
    RevealFile,
    BuiltIn,
}

public sealed record QuickSearchAction(
    QuickSearchActionKind Kind,
    string Value,
    QuickSearchBuiltInAction? BuiltInAction = null);

public sealed record QuickSearchResult(
    string Id,
    QuickSearchResultKind Kind,
    string Title,
    string? Subtitle,
    string Glyph,
    double Score,
    QuickSearchAction Action);

public static class QuickSearchRanking
{
    public static double? Score(
        string query,
        string title,
        string? subtitle = null,
        double providerWeight = 0,
        double recencyBoost = 0,
        double frequencyBoost = 0)
    {
        var normalizedQuery = Normalize(query);
        if (normalizedQuery.Length == 0)
            return providerWeight + recencyBoost + frequencyBoost;

        var titleScore = TextualScore(normalizedQuery, Normalize(title));
        var subtitleMatch = TextualScore(normalizedQuery, Normalize(subtitle ?? string.Empty));
        var subtitleScore = subtitleMatch is null ? null : subtitleMatch * 0.45;
        var textScore = Math.Max(titleScore ?? double.MinValue, subtitleScore ?? double.MinValue);
        if (textScore == double.MinValue) return null;
        return providerWeight + textScore + Math.Min(80, recencyBoost) + Math.Min(45, frequencyBoost);
    }

    public static IReadOnlyList<QuickSearchResult> Sort(IEnumerable<QuickSearchResult> results) =>
        results.OrderByDescending(static value => value.Score)
            .ThenBy(static value => value.Title, StringComparer.CurrentCultureIgnoreCase)
            .ToArray();

    private static double? TextualScore(string query, string candidate)
    {
        if (candidate.Length == 0) return null;
        if (candidate == query) return 520;
        if (candidate.StartsWith(query, StringComparison.Ordinal))
            return 390 - ((candidate.Length - query.Length) * 0.15);
        if (candidate.Contains($" {query}", StringComparison.Ordinal)
            || candidate.Contains($"-{query}", StringComparison.Ordinal)
            || candidate.Contains($"_{query}", StringComparison.Ordinal))
            return 310;
        var substring = candidate.IndexOf(query, StringComparison.Ordinal);
        if (substring >= 0) return 235 - (substring * 0.5);

        var candidateIndex = 0;
        var previousMatch = -1;
        var gapPenalty = 0d;
        foreach (var character in query)
        {
            var match = candidate.IndexOf(character, candidateIndex);
            if (match < 0) return null;
            gapPenalty += previousMatch >= 0 ? (match - previousMatch - 1) * 3 : match * 2;
            previousMatch = match;
            candidateIndex = match + 1;
        }
        return Math.Max(40, 170 - gapPenalty);
    }

    private static string Normalize(string value) =>
        value.Trim().ToLowerInvariant().Normalize(System.Text.NormalizationForm.FormKC);
}
