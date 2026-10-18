using ChatOS.Presentation.Markdown;

namespace ChatOS.Presentation.Tests;

public sealed class MarkdownCodeSpanTests
{
    [Theory]
    [InlineData("`value`", "value")]
    [InlineData("``one`two``", "one`two")]
    [InlineData("```one``two`three```", "one``two`three")]
    [InlineData("``one```two`three``", "one```two`three")]
    [InlineData("`` `tick` ``", "`tick`")]
    [InlineData("``**bold** _italic_ [link](https://example.test) \\*``", "**bold** _italic_ [link](https://example.test) \\*")]
    [InlineData("`中文😀`", "中文😀")]
    [InlineData("`&amp; <tag>`", "&amp; <tag>")]
    public void MatchingRunsKeepInnerBackticksAndMarkupLiteral(string source, string expected)
    {
        var span = Assert.Single(MarkdownInlineParser.Parse(source));
        Assert.Equal(MarkdownInlineKind.Code, span.Kind);
        Assert.Equal(expected, span.Text);
        Assert.Null(span.NavigateUri);
    }

    [Theory]
    [InlineData("` one `", "one")]
    [InlineData("`  one  `", " one ")]
    [InlineData("` one`", " one")]
    [InlineData("`one `", "one ")]
    [InlineData("`   `", "   ")]
    [InlineData("` \t `", "\t")]
    [InlineData("`\tcode\t`", "\tcode\t")]
    [InlineData("`one\ntwo`", "one two")]
    [InlineData("`one\r\ntwo`", "one two")]
    [InlineData("`one\rtwo`", "one two")]
    public void CodeWhitespaceUsesSingleBoundarySpaceAndLineEndingRules(string source, string expected)
    {
        var span = Assert.Single(MarkdownInlineParser.Parse(source));
        Assert.Equal(MarkdownInlineKind.Code, span.Kind);
        Assert.Equal(expected, span.Text);
    }

    [Theory]
    [InlineData("``unclosed`")]
    [InlineData("`longer``")]
    [InlineData("```shorter``")]
    [InlineData("``")]
    [InlineData("`")]
    [InlineData("`a``b```c")]
    public void UnmatchedOrDifferentLengthDelimitersRemainLiteral(string source)
    {
        var spans = MarkdownInlineParser.Parse(source).ToArray();
        Assert.All(spans, span => Assert.Equal(MarkdownInlineKind.Text, span.Kind));
        Assert.Equal(source, string.Concat(spans.Select(span => span.Text)));
    }

    [Fact]
    public void NeighboringStylesAndCodeSpansKeepTheirOwnBoundaries()
    {
        var spans = MarkdownInlineParser.Parse("Before ``a`b`` **bold** `second` _italic_ [link](https://example.test). ").ToArray();
        Assert.Equal("Before a`b bold second italic link. ", string.Concat(spans.Select(span => span.Text)));
        Assert.Equal(["a`b", "second"], spans.Where(span => span.Kind == MarkdownInlineKind.Code).Select(span => span.Text));
        Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Bold);
        Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Italic);
        Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Link);
    }

    [Fact]
    public void EscapeConsumesOneBacktickBeforeOpeningAnotherRun()
    {
        var spans = MarkdownInlineParser.Parse("\\``code`").ToArray();
        Assert.Equal("`code", string.Concat(spans.Select(span => span.Text)));
        Assert.Equal("code", Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Code).Text);
        var escaped = MarkdownInlineParser.Parse("\\`literal\\`").ToArray();
        Assert.All(escaped, span => Assert.Equal(MarkdownInlineKind.Text, span.Kind));
        Assert.Equal("`literal`", string.Concat(escaped.Select(span => span.Text)));
    }

    [Fact]
    public void BackslashInsideCodeDoesNotEscapeClosingDelimiter()
    {
        var spans = MarkdownInlineParser.Parse("`a\\`b`").ToArray();
        Assert.Equal("a\\", Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Code).Text);
        Assert.Equal("a\\b`", string.Concat(spans.Select(span => span.Text)));
    }

    [Fact]
    public void StreamingCompletionReplacesLiteralRunWithoutStaleState()
    {
        Assert.DoesNotContain(MarkdownInlineParser.Parse("``a`b"), span => span.Kind == MarkdownInlineKind.Code);
        var complete = Assert.Single(MarkdownInlineParser.Parse("``a`b``"));
        Assert.Equal(MarkdownInlineKind.Code, complete.Kind);
        Assert.Equal("a`b", complete.Text);
        Assert.DoesNotContain(MarkdownInlineParser.Parse("``a`b"), span => span.Kind == MarkdownInlineKind.Code);
    }
}
