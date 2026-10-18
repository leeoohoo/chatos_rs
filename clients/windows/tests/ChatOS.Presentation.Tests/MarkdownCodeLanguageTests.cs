using ChatOS.Presentation.Markdown;

namespace ChatOS.Presentation.Tests;

public sealed class MarkdownCodeLanguageTests
{
    [Theory]
    [InlineData("```csharp", "```", "csharp", "CSHARP")]
    [InlineData("  ~~~~python  ", "~~~~", "python", "PYTHON")]
    [InlineData("````objective-c\t", "`````", "objective-c", "OBJECTIVE-C")]
    [InlineData("~~~ c++", "~~~", "c++", "C++")]
    public void OpeningFencePreservesLanguageAndProvidesDisplayLabel(
        string opening, string closing, string expectedLanguage, string expectedLabel)
    {
        var block = Assert.Single(MarkdownLineParser.ParseBlocks($"{opening}\n    value\n{closing}"));
        Assert.True(block.IsCode);
        Assert.Equal(expectedLanguage, block.Language);
        Assert.Equal(expectedLabel, block.LanguageLabel);
        Assert.Equal(["    value"], block.Lines);
    }

    [Fact]
    public void LanguageDoesNotLeakToAdjacentUnlabelledCodeOrProse()
    {
        var blocks = MarkdownLineParser.ParseBlocks("```json\n{}\n```\n~~~\nplain\n~~~\nAfter").ToArray();
        Assert.Equal(3, blocks.Length);
        Assert.Equal("JSON", blocks[0].LanguageLabel);
        Assert.Null(blocks[1].Language);
        Assert.Null(blocks[1].LanguageLabel);
        Assert.Null(blocks[2].LanguageLabel);
        Assert.False(blocks[2].IsCode);
    }

    [Fact]
    public void EmptyLabelledBlockRetainsLanguageWithoutInventingABodyLine()
    {
        var block = Assert.Single(MarkdownLineParser.ParseBlocks("```sql\n```"));
        Assert.Equal("SQL", block.LanguageLabel);
        Assert.Empty(block.Lines);
    }

    [Theory]
    [InlineData("```json", new string[0])]
    [InlineData("```json\n{", new[] { "{" })]
    [InlineData("```json\n{\n", new[] { "{", "" })]
    public void StreamingUnclosedCodeRetainsLanguageAndExactBody(string source, string[] expected)
    {
        var block = Assert.Single(MarkdownLineParser.ParseBlocks(source));
        Assert.Equal("JSON", block.LanguageLabel);
        Assert.Equal(expected, block.Lines);
    }

    [Theory]
    [InlineData("```\nbody\n```")]
    [InlineData("~~~   \nbody\n~~~")]
    [InlineData("````\t\nbody\n````")]
    public void MissingOrWhitespaceInfoDoesNotAddALabel(string source)
    {
        var block = Assert.Single(MarkdownLineParser.ParseBlocks(source));
        Assert.True(block.IsCode);
        Assert.Null(block.Language);
        Assert.Null(block.LanguageLabel);
        Assert.Equal(["body"], block.Lines);
    }

    [Fact]
    public void InfoLabelIsLiteralAndDoesNotBecomePartOfTheCodeBody()
    {
        const string source = "```**csharp** [docs](https://example.test)\nvar _name_ = 1;\n```";
        var block = Assert.Single(MarkdownLineParser.ParseBlocks(source));
        Assert.Equal("**CSHARP** [DOCS](HTTPS://EXAMPLE.TEST)", block.LanguageLabel);
        Assert.Equal(["var _name_ = 1;"], block.Lines);
        Assert.Equal(new MarkdownLine("var _name_ = 1;", true), Assert.Single(MarkdownLineParser.Parse(source)));
    }

    [Fact]
    public void InvalidBacktickInfoRemainsProseWithoutLanguageMetadata()
    {
        const string source = "```a`b";
        var block = Assert.Single(MarkdownLineParser.ParseBlocks(source));
        Assert.False(block.IsCode);
        Assert.Null(block.LanguageLabel);
        Assert.Equal([source], block.Lines);
    }
}
