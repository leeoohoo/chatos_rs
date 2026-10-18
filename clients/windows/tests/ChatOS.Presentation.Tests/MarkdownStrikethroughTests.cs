using ChatOS.Presentation.Markdown;

namespace ChatOS.Presentation.Tests;

public sealed class MarkdownStrikethroughTests
{
    [Theory]
    [InlineData("~~old~~", "old")]
    [InlineData("~~旧版本😀~~", "旧版本😀")]
    [InlineData("~~two words~~", "two words")]
    [InlineData("~~two  words\tinside~~", "two  words\tinside")]
    [InlineData("~~one~two~~", "one~two")]
    [InlineData("~~a\\~b~~", "a~b")]
    [InlineData("~~a\\~~b~~", "a~~b")]
    public void DoubleTildesCreateStrikethroughAndPreserveText(string source, string expected)
    {
        var span = Assert.Single(MarkdownInlineParser.Parse(source));
        Assert.Equal(MarkdownInlineKind.Strikethrough, span.Kind);
        Assert.Equal(expected, span.Text);
        Assert.Null(span.NavigateUri);
    }

    [Theory]
    [InlineData("~old~")]
    [InlineData("~~~old~~~")]
    [InlineData("~~~old~~")]
    [InlineData("~~old~~~")]
    [InlineData("~~~~")]
    [InlineData("~~")]
    [InlineData("~~unclosed")]
    [InlineData("~~ old~~")]
    [InlineData("~~old ~~")]
    [InlineData("~~\told~~")]
    [InlineData("~~old\nnext~~")]
    [InlineData("~~old\r\nnext~~")]
    public void InvalidOrIncompletePairsStayLiteral(string source)
    {
        var spans = MarkdownInlineParser.Parse(source).ToArray();
        Assert.All(spans, span => Assert.Equal(MarkdownInlineKind.Text, span.Kind));
        Assert.Equal(source, string.Concat(spans.Select(span => span.Text)));
    }

    [Theory]
    [InlineData("\\~~old~~", "~~old~~")]
    [InlineData("\\~\\~old\\~\\~", "~~old~~")]
    [InlineData("~~old\\~~", "~~old~~")]
    public void EscapedDelimitersDoNotPair(string source, string expected)
    {
        var spans = MarkdownInlineParser.Parse(source).ToArray();
        Assert.All(spans, span => Assert.Equal(MarkdownInlineKind.Text, span.Kind));
        Assert.Equal(expected, string.Concat(spans.Select(span => span.Text)));
    }

    [Theory]
    [InlineData("`~~old~~`")]
    [InlineData("``~~old~~``")]
    public void InlineCodeKeepsTildesLiteral(string source)
    {
        var span = Assert.Single(MarkdownInlineParser.Parse(source));
        Assert.Equal(MarkdownInlineKind.Code, span.Kind);
        Assert.Equal("~~old~~", span.Text);
    }

    [Fact]
    public void NeighboringStylesKeepTheirOwnBoundaries()
    {
        var spans = MarkdownInlineParser.Parse("Before ~~old~~ ~~older~~ **bold** _italic_ `code` [link](https://example.test).").ToArray();
        Assert.Equal("Before old older bold italic code link.", string.Concat(spans.Select(span => span.Text)));
        Assert.Equal(["old", "older"], spans.Where(span => span.Kind == MarkdownInlineKind.Strikethrough).Select(span => span.Text));
        Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Bold);
        Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Italic);
        Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Code);
        Assert.Equal("https://example.test", Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Link).NavigateUri!.OriginalString);
    }

    [Fact]
    public void EscapedBackslashDoesNotEscapeTheFollowingPair()
    {
        var spans = MarkdownInlineParser.Parse("\\\\~~old~~").ToArray();
        Assert.Equal("\\old", string.Concat(spans.Select(span => span.Text)));
        Assert.Equal("old", Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Strikethrough).Text);
    }

    [Fact]
    public void StreamingCompletionReplacesLiteralDelimitersWithoutStaleState()
    {
        Assert.DoesNotContain(MarkdownInlineParser.Parse("~~old"), span => span.Kind == MarkdownInlineKind.Strikethrough);
        var complete = Assert.Single(MarkdownInlineParser.Parse("~~old~~"));
        Assert.Equal(MarkdownInlineKind.Strikethrough, complete.Kind);
        Assert.Equal("old", complete.Text);
        Assert.DoesNotContain(MarkdownInlineParser.Parse("~~old"), span => span.Kind == MarkdownInlineKind.Strikethrough);
    }

    [Fact]
    public void ProseSoftLinesCanContinueStrikethroughButBlankLinesCannot()
    {
        var paragraph = Assert.Single(MarkdownLineParser.ParseDocumentBlocks("~~old\nversion~~"));
        Assert.Equal("old version", Assert.Single(MarkdownInlineParser.Parse(paragraph.Lines[0])).Text);
        var separated = MarkdownLineParser.ParseDocumentBlocks("~~old\n\nversion~~").ToArray();
        Assert.Equal(2, separated.Length);
        Assert.All(separated, block => Assert.DoesNotContain(MarkdownInlineParser.Parse(block.Lines[0]),
            span => span.Kind == MarkdownInlineKind.Strikethrough));
    }

    [Fact]
    public void ManyUnmatchedPairsKeepSourceTextAcrossLineBarriers()
    {
        var source = string.Concat(Enumerable.Repeat("~~unfinished\n", 4000)) + "end~~";
        var spans = MarkdownInlineParser.Parse(source).ToArray();
        Assert.DoesNotContain(spans, span => span.Kind == MarkdownInlineKind.Strikethrough);
        Assert.Equal(source, string.Concat(spans.Select(span => span.Text)));
    }
}
