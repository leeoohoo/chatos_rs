namespace ChatOS.Presentation.Markdown;

public sealed record MarkdownHeading(int Level, string Text);

/// <summary>The ATX heading subset used by the shared native reader.</summary>
public static class MarkdownHeadingParser
{
    public static MarkdownHeading? Parse(string? source)
    {
        if (string.IsNullOrWhiteSpace(source) || source.IndexOfAny(['\r', '\n']) >= 0) return null;
        var line = source.Trim();
        var level = 0;
        while (level < line.Length && line[level] == '#') level++;
        if (level is < 1 or > 6 || level >= line.Length || !char.IsWhiteSpace(line[level])) return null;
        var text = line[level..].Trim();
        return text.Length > 0 ? new MarkdownHeading(level, text) : null;
    }
}
