using ChatOS.Presentation.Markdown;

namespace ChatOS.Presentation.Tests;

public sealed class MarkdownLineParserTests
{
    [Fact]
    public void CodeKeepsMarkdownSyntaxIndentationAndBlankLines()
    {
        const string code = "    **literal** _identifier_ `backticks` [link](https://example.com)";

        var lines = MarkdownLineParser.Parse($"Before\n```text\n{code}\n\n```\nAfter").ToArray();

        Assert.Equal(new[]
        {
            new MarkdownLine("Before", false),
            new MarkdownLine(code, true),
            new MarkdownLine(string.Empty, true),
            new MarkdownLine("After", false),
        }, lines);
    }

    [Fact]
    public void ShorterDifferentOrSuffixedFencesStayInsideCode()
    {
        var lines = MarkdownLineParser.Parse(
            "````markdown\n```\n~~~\n````still code\n`````\nAfter").ToArray();

        Assert.Equal(new[]
        {
            new MarkdownLine("```", true),
            new MarkdownLine("~~~", true),
            new MarkdownLine("````still code", true),
            new MarkdownLine("After", false),
        }, lines);
    }

    [Fact]
    public void UnclosedStreamingBlockKeepsItsLastLineAsCode()
    {
        var line = Assert.Single(MarkdownLineParser.Parse("```csharp\nvar _name_ = \"**value**\";"));

        Assert.Equal(new MarkdownLine("var _name_ = \"**value**\";", true), line);
    }

    [Fact]
    public void TildeFenceCanContainBackticksAndCloseWithWhitespace()
    {
        var lines = MarkdownLineParser.Parse("  ~~~text\n```\n  ~~~~ \t\nAfter").ToArray();

        Assert.Equal(new[]
        {
            new MarkdownLine("```", true),
            new MarkdownLine("After", false),
        }, lines);
    }

    [Theory]
    [InlineData("\r\n")]
    [InlineData("\r")]
    [InlineData("\n")]
    public void NormalizesLineEndings(string newline)
    {
        var lines = MarkdownLineParser.Parse($"```{newline}one{newline}two{newline}```");

        Assert.Equal(new[] { new MarkdownLine("one", true), new MarkdownLine("two", true) }, lines);
    }

    [Fact]
    public void InlineBackticksDoNotOpenABlock()
    {
        var line = Assert.Single(MarkdownLineParser.Parse("```inline```"));

        Assert.Equal(new MarkdownLine("```inline```", false), line);
    }
}
