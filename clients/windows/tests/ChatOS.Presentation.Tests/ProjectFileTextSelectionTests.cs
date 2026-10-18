using ChatOS.Presentation.Projects;

namespace ChatOS.Presentation.Tests;

public sealed class ProjectFileTextSelectionTests
{
    [Theory]
    [InlineData("\n")]
    [InlineData("\r\n")]
    [InlineData("\r")]
    public void LocationAccountsForBlankLinesAndExcludesLineEndings(string newline)
    {
        var text = $"first{newline}{newline}target{newline}last";

        var selection = ProjectFileTextSelection.ForLocation(text, 3, 2);

        Assert.Equal("arget", text.Substring(selection.Start, selection.Length));
        Assert.Equal(text.IndexOf("target", StringComparison.Ordinal) + 1, selection.Start);
    }

    [Theory]
    [InlineData("first\r\nlast", int.MaxValue, int.MaxValue, 11, 0)]
    [InlineData("first\r\nlast", int.MaxValue, 1, 7, 4)]
    [InlineData("first\r\nlast", 0, -4, 0, 5)]
    [InlineData("first\r\nlast", 1, 100, 5, 0)]
    [InlineData("first\r\n\r\nlast", 2, 8, 7, 0)]
    [InlineData("first\r\n", 2, 1, 7, 0)]
    [InlineData("", 1, 1, 0, 0)]
    public void UnavailableLocationsStayWithinTheNearestLine(
        string text, int line, int column, int expectedStart, int expectedLength)
    {
        var selection = ProjectFileTextSelection.ForLocation(text, line, column);

        Assert.Equal(expectedStart, selection.Start);
        Assert.Equal(expectedLength, selection.Length);
    }
}
