using System.Text.RegularExpressions;

namespace ChatOS.Presentation.Markdown;

public enum MarkdownInlineKind { Text, Bold, Code, Italic, Link, Strikethrough }

public sealed record MarkdownInlineSpan(string Text, MarkdownInlineKind Kind, Uri? NavigateUri = null);

/// <summary>The inline subset used by the native Windows Markdown reader.</summary>
public static partial class MarkdownInlineParser
{
    public static IEnumerable<MarkdownInlineSpan> Parse(string? source)
    {
        var text = source ?? string.Empty;
        var cursor = 0;
        Dictionary<int, List<int>>? codeRuns = null;
        List<int>? strikeBoundaries = null;
        while (cursor < text.Length)
        {
            var match = InlineTokenPattern().Match(text, cursor);
            if (!match.Success) break;
            if (match.Index > cursor)
                yield return new(text[cursor..match.Index], MarkdownInlineKind.Text);

            cursor = match.Index + match.Length;
            if (match.Groups["escape"].Success)
                yield return new(match.Value[1..], MarkdownInlineKind.Text);
            else if (match.Groups["bold"].Success)
                yield return new(match.Groups["bold"].Value, MarkdownInlineKind.Bold);
            else if (match.Groups["codeOpen"].Success)
            {
                codeRuns ??= IndexCodeRuns(text);
                if (TryReadCodeSpan(text, match.Index, match.Length, codeRuns, out var codeEnd, out var code))
                {
                    cursor = codeEnd;
                    yield return new(code, MarkdownInlineKind.Code);
                }
                else
                    yield return new(match.Value, MarkdownInlineKind.Text);
            }
            else if (match.Groups["italic"].Success)
                yield return new(match.Groups["italic"].Value, MarkdownInlineKind.Italic);
            else if (match.Groups["strikeOpen"].Success)
            {
                if (match.Length == 2 && cursor < text.Length && !char.IsWhiteSpace(text[cursor]))
                {
                    strikeBoundaries ??= IndexStrikeBoundaries(text);
                    var next = strikeBoundaries.BinarySearch(cursor + 1);
                    if (next < 0) next = ~next;
                    if (next < strikeBoundaries.Count && text[strikeBoundaries[next]] == '~')
                    {
                        var closing = strikeBoundaries[next];
                        yield return new(Unescape(text[cursor..closing]), MarkdownInlineKind.Strikethrough);
                        cursor = closing + 2;
                        continue;
                    }
                }
                yield return new(match.Value, MarkdownInlineKind.Text);
            }
            else if (TryReadLink(text, match.Index, out var end, out var label, out var destination))
            {
                cursor = end;
                if (Uri.TryCreate(Unescape(destination), UriKind.Absolute, out var uri) &&
                    uri.Scheme is "http" or "https" or "mailto")
                    yield return new(Unescape(label), MarkdownInlineKind.Link, uri);
                else
                    yield return new(text[match.Index..end], MarkdownInlineKind.Text);
            }
            else
                yield return new(match.Value, MarkdownInlineKind.Text);
        }

        if (cursor < text.Length) yield return new(text[cursor..], MarkdownInlineKind.Text);
    }

    private static List<int> IndexStrikeBoundaries(string text)
    {
        // Include line ends as barriers, so unmatched streaming openers never
        // rescan the remaining text. Only exact, unescaped double runs close.
        var boundaries = new List<int>();
        for (var index = 0; index < text.Length;)
        {
            if (text[index] == '\\' && index + 1 < text.Length && IsEscapable(text[index + 1]))
                index += 2;
            else if (text[index] == '~')
            {
                var start = index++;
                while (index < text.Length && text[index] == '~') index++;
                if (index - start == 2 && start > 0 && !char.IsWhiteSpace(text[start - 1]))
                    boundaries.Add(start);
            }
            else
            {
                if (text[index] is '\r' or '\n') boundaries.Add(index);
                index++;
            }
        }
        return boundaries;
    }

    private static Dictionary<int, List<int>> IndexCodeRuns(string text)
    {
        // Index maximal runs once. An unmatched streaming opener must not scan
        // the remaining document again for every later backtick token.
        var runs = new Dictionary<int, List<int>>();
        var index = 0;
        while ((index = text.IndexOf('`', index)) >= 0)
        {
            var start = index++;
            while (index < text.Length && text[index] == '`') index++;
            var length = index - start;
            if (!runs.TryGetValue(length, out var positions))
                runs[length] = positions = [];
            positions.Add(start);
        }
        return runs;
    }

    private static bool TryReadCodeSpan(
        string text, int start, int length, Dictionary<int, List<int>> runs,
        out int end, out string content)
    {
        end = start;
        content = string.Empty;
        if (!runs.TryGetValue(length, out var positions)) return false;
        var next = positions.BinarySearch(start + length);
        if (next < 0) next = ~next;
        if (next >= positions.Count) return false;

        var closing = positions[next];
        end = closing + length;
        content = text[(start + length)..closing]
            .Replace("\r\n", " ").Replace('\r', ' ').Replace('\n', ' ');
        // Strip one boundary space only when both ends have a space and the
        // span contains something else. Tabs and all-space spans stay literal.
        if (content.Length >= 2 && content[0] == ' ' && content[^1] == ' ' &&
            content.Any(character => character != ' '))
            content = content[1..^1];
        return true;
    }

    private static bool TryReadLink(
        string text, int start, out int end, out string label, out string destination)
    {
        end = start;
        label = destination = string.Empty;
        var index = start + 1;
        var depth = 1;
        for (; index < text.Length; index++)
        {
            var character = text[index];
            if (character is '\r' or '\n') return false;
            if (character == '\\' && index + 1 < text.Length && IsEscapable(text[index + 1]))
            {
                index++;
                continue;
            }
            if (character == '[' && ++depth > 32) return false;
            if (character == ']' && --depth == 0) break;
        }
        if (index >= text.Length || index == start + 1 ||
            index + 1 >= text.Length || text[index + 1] != '(') return false;
        label = text[(start + 1)..index];
        index += 2;
        SkipSpace(text, ref index);
        if (index >= text.Length) return false;

        var destinationStart = index;
        if (text[index] == '<')
        {
            destinationStart = ++index;
            for (; index < text.Length && text[index] != '>'; index++)
            {
                if (text[index] is '\r' or '\n' or '<') return false;
                if (text[index] == '\\' && index + 1 < text.Length && IsEscapable(text[index + 1])) index++;
            }
            if (index >= text.Length) return false;
            destination = text[destinationStart..index++];
        }
        else
        {
            depth = 0;
            for (; index < text.Length; index++)
            {
                var character = text[index];
                if (character == '\\' && index + 1 < text.Length && IsEscapable(text[index + 1]))
                {
                    index++;
                    continue;
                }
                if (char.IsWhiteSpace(character) || character == '<') break;
                if (character == '(' && ++depth > 32) return false;
                if (character == ')')
                {
                    if (depth == 0) break;
                    depth--;
                }
            }
            if (depth != 0) return false;
            destination = text[destinationStart..index];
        }
        if (destination.Length == 0) return false;

        var beforeSpace = index;
        SkipSpace(text, ref index);
        if (index >= text.Length) return false;
        if (text[index] != ')')
        {
            if (index == beforeSpace || text[index] is not ('"' or '\'' or '(')) return false;
            var closingQuote = text[index] == '(' ? ')' : text[index];
            index++;
            for (; index < text.Length && text[index] != closingQuote; index++)
            {
                if (text[index] is '\r' or '\n') return false;
                if (text[index] == '\\' && index + 1 < text.Length && IsEscapable(text[index + 1])) index++;
                else if (closingQuote == ')' && text[index] == '(') return false;
            }
            if (index >= text.Length) return false;
            index++;
            SkipSpace(text, ref index);
        }
        if (index >= text.Length || text[index] != ')') return false;
        end = index + 1;
        return true;
    }

    private static void SkipSpace(string text, ref int index)
    {
        while (index < text.Length && text[index] is ' ' or '\t') index++;
    }

    private static bool IsEscapable(char character) =>
        character is >= '!' and <= '/' or >= ':' and <= '@' or >= '[' and <= '`' or >= '{' and <= '~';

    private static string Unescape(string text) => EscapePattern().Replace(text, "$1");

    [GeneratedRegex(@"\\(?<escape>[!""#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~])|\*\*(?<bold>.+?)\*\*|(?<codeOpen>`+)|(?<!\*)\*(?<italic>[^*\n]+)\*(?!\*)|(?<!_)_(?<italic>[^_\n]+)_(?!_)|(?<strikeOpen>~+)|\[")]
    private static partial Regex InlineTokenPattern();

    [GeneratedRegex(@"\\([!""#$%&'()*+,\-./:;<=>?@\[\]\\^_`{|}~])")]
    private static partial Regex EscapePattern();
}
