namespace ChatOS.Presentation.Projects;

public static class ProjectFileTextSelection
{
    // Use the native TextBox text: Windows may normalize file line endings.
    // Select from the requested column to the end of its line, excluding CR/LF.
    public static (int Start, int Length) ForLocation(string text, int line, int column)
    {
        var start = 0;
        var currentLine = 1;
        while (currentLine < line && start < text.Length)
        {
            var end = start;
            while (end < text.Length && text[end] is not ('\r' or '\n')) end++;
            if (end == text.Length) break;

            start = end + 1;
            if (text[end] == '\r' && start < text.Length && text[start] == '\n') start++;
            currentLine++;
        }

        var lineEnd = start;
        while (lineEnd < text.Length && text[lineEnd] is not ('\r' or '\n')) lineEnd++;
        var offset = Math.Clamp(column, 1, Math.Max(1, lineEnd - start + 1)) - 1;
        return (start + offset, lineEnd - start - offset);
    }
}
