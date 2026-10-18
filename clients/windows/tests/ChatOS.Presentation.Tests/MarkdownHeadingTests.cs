using ChatOS.Presentation.Markdown;

namespace ChatOS.Presentation.Tests;

public sealed class MarkdownHeadingTests
{
    [Theory]
    [InlineData(1)]
    [InlineData(2)]
    [InlineData(3)]
    [InlineData(4)]
    [InlineData(5)]
    [InlineData(6)]
    public void OneThroughSixMarkersProduceTheCorrectHeadingLevel(int level)
    {
        var heading = Assert.IsType<MarkdownHeading>(MarkdownHeadingParser.Parse($"{new string('#', level)} 标题 {level}"));
        Assert.Equal(level, heading.Level);
        Assert.Equal($"标题 {level}", heading.Text);
    }

    [Theory]
    [InlineData("  ####\t标题  ")]
    [InlineData("\t####    标题\t")]
    [InlineData("####\u2003标题")]
    [InlineData("####\u00a0标题")]
    public void HeadingWhitespaceIsAcceptedAndOnlyBoundaryWhitespaceIsRemoved(string source)
    {
        Assert.Equal(new MarkdownHeading(4, "标题"), MarkdownHeadingParser.Parse(source));
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData(" \t")]
    [InlineData("#")]
    [InlineData("###### \t")]
    [InlineData("####### title")]
    [InlineData("####title")]
    [InlineData("\\#### title")]
    [InlineData("ordinary # title")]
    [InlineData("#\ntitle")]
    [InlineData("# title\r\nnext")]
    public void EmptyEscapedOverlongOrUnseparatedMarkersAreNotHeadings(string? source) =>
        Assert.Null(MarkdownHeadingParser.Parse(source));

    [Theory]
    [InlineData("#### **Bold** and `code`", "**Bold** and `code`")]
    [InlineData("##### [Document](https://example.test/a_(b))", "[Document](https://example.test/a_(b))")]
    [InlineData("###### Title ##", "Title ##")]
    [InlineData("#### Two  words\there", "Two  words\there")]
    public void HeadingBodyKeepsInlineSourceAndMacClosingMarkerBehavior(string source, string expected)
    {
        var heading = Assert.IsType<MarkdownHeading>(MarkdownHeadingParser.Parse(source));
        Assert.Equal(expected, heading.Text);
    }

    [Fact]
    public void AllHeadingLevelsSeparateNeighboringProseParagraphs()
    {
        var source = string.Join("\n", Enumerable.Range(1, 6)
            .Select(level => $"before {level}\n{new string('#', level)}\tHeading {level}\nafter {level}\n"));
        var blocks = MarkdownLineParser.ParseDocumentBlocks(source).ToArray();
        Assert.Equal(18, blocks.Length);
        for (var index = 0; index < 6; index++)
        {
            var heading = Assert.IsType<MarkdownHeading>(MarkdownHeadingParser.Parse(Assert.Single(blocks[index * 3 + 1].Lines)));
            Assert.Equal(index + 1, heading.Level);
            Assert.Equal($"Heading {index + 1}", heading.Text);
        }
    }

    [Fact]
    public void InvalidAndEmptyHeadingMarkersStayInTheirOrdinaryParagraph()
    {
        var paragraph = Assert.Single(MarkdownLineParser.ParseDocumentBlocks("##### \n####### literal\n\\#### escaped\nafter"));
        Assert.Equal(["##### ####### literal \\#### escaped after"], paragraph.Lines);
        Assert.True(paragraph.IsProse);
    }

    [Theory]
    [InlineData("#")]
    [InlineData("##")]
    [InlineData("######")]
    [InlineData("-")]
    [InlineData("+")]
    [InlineData("*")]
    [InlineData("12.")]
    public void JoiningOrdinaryLinesCannotCreateANewStructuralMarker(string marker)
    {
        var block = Assert.Single(MarkdownLineParser.ParseDocumentBlocks($"{marker}\nnext line"));
        Assert.True(block.IsProse);
        Assert.Equal([$"{marker} next line"], block.Lines);
    }

    [Fact]
    public void HeadingTextInsideFencedCodeRemainsLiteral()
    {
        var block = Assert.Single(MarkdownLineParser.ParseDocumentBlocks("```markdown\n#### title\n######\t**literal**\n```"));
        Assert.True(block.IsCode);
        Assert.Equal(["#### title", "######\t**literal**"], block.Lines);
    }
}
