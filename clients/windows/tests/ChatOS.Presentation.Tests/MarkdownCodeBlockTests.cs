using ChatOS.Presentation.Markdown;

namespace ChatOS.Presentation.Tests;

public sealed class MarkdownCodeBlockTests
{
    [Fact]
    public void CodeIsOneBlockWithLiteralIndentationBlankLinesAndTrailingSpaces()
    {
        var blocks = MarkdownLineParser.ParseBlocks("Before\n```csharp\n    var _x_ = \"**literal**\";  \n\n\treturn _x_;\n```\nAfter").ToArray();

        Assert.Equal(3, blocks.Length);
        Assert.False(blocks[0].IsCode);
        Assert.Equal(["Before"], blocks[0].Lines);
        Assert.True(blocks[1].IsCode);
        Assert.Equal(["    var _x_ = \"**literal**\";  ", "", "\treturn _x_;"], blocks[1].Lines);
        Assert.False(blocks[2].IsCode);
        Assert.Equal(["After"], blocks[2].Lines);
    }

    [Fact]
    public void AdjacentFencedBlocksRetainSeparateBoundaries()
    {
        var blocks = MarkdownLineParser.ParseBlocks("```one\nfirst\n```\n~~~two\nsecond\n~~~").ToArray();

        Assert.Equal(2, blocks.Length);
        Assert.All(blocks, block => Assert.True(block.IsCode));
        Assert.Equal(["first"], blocks[0].Lines);
        Assert.Equal(["second"], blocks[1].Lines);
    }

    [Fact]
    public void EmptyAndBlankLineCodeBlocksRemainDistinct()
    {
        var blocks = MarkdownLineParser.ParseBlocks("```\n```\n```\n\n```").ToArray();

        Assert.Equal(2, blocks.Length);
        Assert.Empty(blocks[0].Lines);
        Assert.Equal([""], blocks[1].Lines);
    }

    [Fact]
    public void NonClosingMarkersRemainInTheSameLiteralBlock()
    {
        var block = Assert.Single(MarkdownLineParser.ParseBlocks("````markdown\n```\n~~~\n````suffix\ncontent\n`````"));

        Assert.True(block.IsCode);
        Assert.Equal(["```", "~~~", "````suffix", "content"], block.Lines);
    }

    [Theory]
    [InlineData("\n")]
    [InlineData("\r\n")]
    [InlineData("\r")]
    public void NewlineStylesHaveTheSameCodeLines(string newline)
    {
        var block = Assert.Single(MarkdownLineParser.ParseBlocks($"~~~{newline}one{newline}{newline}two  {newline}~~~"));

        Assert.Equal(["one", "", "two  "], block.Lines);
    }

    [Theory]
    [InlineData("```text\nfirst\nlast", new[] { "first", "last" })]
    [InlineData("```text\nfirst\n", new[] { "first", "" })]
    [InlineData("```text", new string[0])]
    public void StreamingUnclosedCodeRemainsOneBlock(string source, string[] expected)
    {
        var block = Assert.Single(MarkdownLineParser.ParseBlocks(source));

        Assert.True(block.IsCode);
        Assert.Equal(expected, block.Lines);
    }

    [Fact]
    public void ClosingOneBlockDoesNotMutateItsLinesWhenTheNextIsRead()
    {
        using var blocks = MarkdownLineParser.ParseBlocks("```\nfirst\n```\n```\nsecond\n```").GetEnumerator();
        Assert.True(blocks.MoveNext());
        var first = blocks.Current;
        Assert.True(blocks.MoveNext());
        Assert.Equal(["first"], first.Lines);
        Assert.Equal(["second"], blocks.Current.Lines);
        Assert.False(blocks.MoveNext());
    }

    [Fact]
    public void ProseBlankLinesAndInlineFenceTextKeepTheirOriginalOrder()
    {
        var blocks = MarkdownLineParser.ParseBlocks("# Title\n\n```inline```\n- item").ToArray();

        Assert.All(blocks, block => Assert.False(block.IsCode));
        Assert.Equal(["# Title", "", "```inline```", "- item"], blocks.SelectMany(block => block.Lines));
    }
}
