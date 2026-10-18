namespace ChatOS.Presentation.Markdown;

public sealed record MarkdownLine(string Text, bool IsCode);

public sealed record MarkdownRenderBlock(IReadOnlyList<string> Lines, bool IsCode, string? Language = null)
{
    public string? LanguageLabel => Language?.ToUpperInvariant();
    public bool IsProse { get; init; }
}

public static class MarkdownLineParser
{
    public static IEnumerable<MarkdownLine> Parse(string? markdown) => ParseBlocks(markdown)
        .SelectMany(static block => block.Lines.Select(text => new MarkdownLine(text, block.IsCode)));

    /// <summary>Joins prose soft lines, retaining the reader's existing structural boundaries.</summary>
    public static IEnumerable<MarkdownRenderBlock> ParseDocumentBlocks(string? markdown)
    {
        var paragraph = new List<string>();
        foreach (var block in ParseBlocks(markdown))
        {
            var line = block.IsCode ? string.Empty : block.Lines[0];
            if (block.IsCode || string.IsNullOrWhiteSpace(line) || IsStandaloneLine(line.Trim()))
            {
                if (paragraph.Count > 0)
                {
                    yield return new MarkdownRenderBlock([string.Join(" ", paragraph)], false) { IsProse = true };
                    paragraph.Clear();
                }
                if (block.IsCode || !string.IsNullOrWhiteSpace(line)) yield return block;
            }
            else
                paragraph.Add(line.Trim());
        }
        if (paragraph.Count > 0)
            yield return new MarkdownRenderBlock([string.Join(" ", paragraph)], false) { IsProse = true };
    }

    private static bool IsStandaloneLine(string line)
    {
        // Keep unsupported image/table source on its own line until native
        // renderers exist; prose joining must not flatten those source rows.
        if (line[0] == '>' || line.StartsWith("![", StringComparison.Ordinal) || line.Contains('|')) return true;
        if (line.Length > 1 && line[0] is '-' or '*' or '+' && char.IsWhiteSpace(line[1])) return true;
        if (line.Length >= 3 && line[0] is '-' or '*' or '_' && line.All(character => character == line[0])) return true;

        if (MarkdownHeadingParser.Parse(line) is not null) return true;

        var index = 0;
        while (index < line.Length && char.IsDigit(line[index])) index++;
        return index > 0 && index + 1 < line.Length && line[index] is '.' or ')' && char.IsWhiteSpace(line[index + 1]);
    }

    public static IEnumerable<MarkdownRenderBlock> ParseBlocks(string? markdown)
    {
        char fenceCharacter = '\0';
        var fenceLength = 0;
        string? language = null;
        var codeLines = new List<string>();
        foreach (var line in (markdown ?? string.Empty)
                     .Replace("\r\n", "\n", StringComparison.Ordinal)
                     .Replace('\r', '\n')
                     .Split('\n'))
        {
            var trimmed = line.TrimStart();
            var markerLength = CountFenceCharacters(trimmed);
            if (fenceLength == 0)
            {
                if (markerLength >= 3 &&
                    (trimmed[0] != '`' || !trimmed[markerLength..].Contains('`')))
                {
                    fenceCharacter = trimmed[0];
                    fenceLength = markerLength;
                    var info = trimmed[markerLength..].Trim();
                    language = info.Length == 0 ? null : info;
                    continue;
                }
            }
            else if (markerLength >= fenceLength &&
                     trimmed[0] == fenceCharacter &&
                     string.IsNullOrWhiteSpace(trimmed[markerLength..]))
            {
                // Snapshot before the iterator advances to another fence. Even
                // adjacent or empty code blocks retain their own boundaries.
                yield return new MarkdownRenderBlock(codeLines.ToArray(), true, language);
                codeLines.Clear();
                fenceCharacter = '\0';
                fenceLength = 0;
                language = null;
                continue;
            }

            if (fenceLength > 0) codeLines.Add(line);
            else yield return new MarkdownRenderBlock([line], false);
        }

        if (fenceLength > 0) yield return new MarkdownRenderBlock(codeLines.ToArray(), true, language);
    }

    private static int CountFenceCharacters(string line)
    {
        if (line.Length == 0 || line[0] is not ('`' or '~')) return 0;

        var length = 1;
        while (length < line.Length && line[length] == line[0]) length++;
        return length;
    }
}
