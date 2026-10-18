using ChatOS.Presentation.Markdown;

namespace ChatOS.Presentation.Tests;

public sealed class MarkdownParagraphTests
{
    [Theory]
    [InlineData("\n")]
    [InlineData("\r\n")]
    [InlineData("\r")]
    public void ProseSoftLinesFormOneParagraphForEachLineEnding(string newline)
    {
        var block = Assert.Single(MarkdownLineParser.ParseDocumentBlocks($"First line{newline}second line{newline}third line"));
        Assert.False(block.IsCode);
        Assert.Equal(["First line second line third line"], block.Lines);
    }

    [Fact]
    public void BlankLinesSeparateParagraphsWithoutEmptyNativeBlocks()
    {
        var blocks = MarkdownLineParser.ParseDocumentBlocks("\n\nfirst\ncontinued\n\n \t\nsecond\n\n").ToArray();
        Assert.Equal(["first continued", "second"], blocks.SelectMany(block => block.Lines));
    }

    [Fact]
    public void ProseTrimsLineBoundariesAndPreservesInnerWhitespace()
    {
        var block = Assert.Single(MarkdownLineParser.ParseDocumentBlocks("  first  value  \n\tsecond\tvalue\t"));
        Assert.Equal(["first  value second\tvalue"], block.Lines);
    }

    [Theory]
    [InlineData("# Heading")]
    [InlineData("## Heading")]
    [InlineData("### Heading")]
    [InlineData("#### Heading")]
    [InlineData("##### Heading")]
    [InlineData("###### Heading")]
    [InlineData("- item")]
    [InlineData("* item")]
    [InlineData("+ item")]
    [InlineData("1. item")]
    [InlineData("12) item")]
    [InlineData("١. item")]
    [InlineData("> quote")]
    [InlineData(">")]
    [InlineData("---")]
    [InlineData("***")]
    [InlineData("___")]
    [InlineData("![image](https://example.test/image.png)")]
    [InlineData("| First | Second |")]
    [InlineData("First | Second")]
    public void StructuralOrPendingImageAndTableLinesDoNotMergeWithProse(string line)
    {
        var blocks = MarkdownLineParser.ParseDocumentBlocks($"before\n{line}\nafter").ToArray();
        Assert.Equal(3, blocks.Length);
        Assert.Equal(["before", line, "after"], blocks.SelectMany(block => block.Lines));
        Assert.All(blocks, block => Assert.False(block.IsCode));
    }

    [Fact]
    public void FencedCodeKeepsLanguageBlankLinesIndentationAndNeighbors()
    {
        var blocks = MarkdownLineParser.ParseDocumentBlocks("before\ncontinued\n```json\n  first  \n\n\tsecond\n```\nafter\ncontinued").ToArray();
        Assert.Equal(3, blocks.Length);
        Assert.Equal(["before continued"], blocks[0].Lines);
        Assert.True(blocks[1].IsCode);
        Assert.Equal("json", blocks[1].Language);
        Assert.Equal(["  first  ", "", "\tsecond"], blocks[1].Lines);
        Assert.Equal(["after continued"], blocks[2].Lines);
    }

    [Fact]
    public void UnclosedStreamingFenceDoesNotJoinWithProse()
    {
        var blocks = MarkdownLineParser.ParseDocumentBlocks("before\n~~~text\n  first\n\nlast\n").ToArray();
        Assert.Equal(2, blocks.Length);
        Assert.Equal(["before"], blocks[0].Lines);
        Assert.True(blocks[1].IsCode);
        Assert.Equal(["  first", "", "last", ""], blocks[1].Lines);
    }

    [Fact]
    public void SoftLineAllowsInlineCodeToContinueWithinTheParagraph()
    {
        var block = Assert.Single(MarkdownLineParser.ParseDocumentBlocks("Use ``one`\n two`` after."));
        var spans = MarkdownInlineParser.Parse(Assert.Single(block.Lines)).ToArray();
        Assert.Equal("one` two", Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Code).Text);
        Assert.Equal("Use one` two after.", string.Concat(spans.Select(span => span.Text)));
    }

    [Fact]
    public void InlineCodeCannotCrossBlankOrStructuralBoundaries()
    {
        var blocks = MarkdownLineParser.ParseDocumentBlocks("``first\n\nsecond``\n# Heading\n``third\n- item\nfourth``").ToArray();
        Assert.Equal(6, blocks.Length);
        Assert.All(blocks, block => Assert.DoesNotContain(
            MarkdownInlineParser.Parse(Assert.Single(block.Lines)), span => span.Kind == MarkdownInlineKind.Code));
    }

    [Fact]
    public void EscapedMarkersAndOrdinaryPunctuationStayInTheParagraph()
    {
        var block = Assert.Single(MarkdownLineParser.ParseDocumentBlocks("before\n\\# literal\n123word\n+literal\n#hashtag"));
        Assert.Equal(["before \\# literal 123word +literal #hashtag"], block.Lines);
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData(" \t\n\n")]
    public void EmptyDocumentsHaveNoVisibleParagraphs(string? source) =>
        Assert.Empty(MarkdownLineParser.ParseDocumentBlocks(source));

    [Fact]
    public void YieldedParagraphIsStableWhileEnumeratorReadsLaterBlocks()
    {
        using var blocks = MarkdownLineParser.ParseDocumentBlocks("first\ncontinued\n\nsecond\ncontinued").GetEnumerator();
        Assert.True(blocks.MoveNext());
        var first = blocks.Current;
        Assert.True(blocks.MoveNext());
        Assert.Equal(["first continued"], first.Lines);
        Assert.Equal(["second continued"], blocks.Current.Lines);
        Assert.False(blocks.MoveNext());
    }

    [Fact]
    public void LegacyLineAndCodeBlockApisKeepOriginalProseLines()
    {
        const string source = "first\nsecond\n\nthird";
        Assert.Equal(["first", "second", "", "third"], MarkdownLineParser.Parse(source).Select(line => line.Text));
        Assert.Equal(["first", "second", "", "third"], MarkdownLineParser.ParseBlocks(source).SelectMany(block => block.Lines));
    }
}
