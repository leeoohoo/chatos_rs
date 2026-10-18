using ChatOS.Presentation.Markdown;

namespace ChatOS.Presentation.Tests;

public sealed class MarkdownInlineParserTests
{
    [Theory]
    [InlineData("[函数](https://example.test/Function_(math))", "https://example.test/Function_(math)")]
    [InlineData("[函数](https://example.test/a_(b_(c)))", "https://example.test/a_(b_(c))")]
    [InlineData("[函数](https://example.test/a\\(b\\))", "https://example.test/a(b)")]
    [InlineData("[函数](https://example.test/a\\)b)", "https://example.test/a)b")]
    [InlineData("[函数](<https://example.test/Function_(math)>)", "https://example.test/Function_(math)")]
    [InlineData("[函数](https://example.test/a%28b%29?q=(x)#section)", "https://example.test/a%28b%29?q=(x)#section")]
    public void CompleteDestinationSurvivesParenthesesAndEscapes(string markdown, string expected)
    {
        var span = Assert.Single(MarkdownInlineParser.Parse(markdown));
        Assert.Equal(MarkdownInlineKind.Link, span.Kind);
        Assert.Equal("函数", span.Text);
        Assert.Equal(expected, span.NavigateUri!.OriginalString);
    }

    [Theory]
    [InlineData("[文档](https://example.test/a_(b) \"Document title\")")]
    [InlineData("[文档](<https://example.test/a_(b)> 'Document title')")]
    [InlineData("[文档]( https://example.test/a_(b) (Document title) )")]
    [InlineData("[文档](https://example.test/a_(b) \"An escaped \\\"title\\\"\")")]
    public void OptionalTitleIsNotPartOfNavigationDestination(string markdown)
    {
        var span = Assert.Single(MarkdownInlineParser.Parse(markdown));
        Assert.Equal(MarkdownInlineKind.Link, span.Kind);
        Assert.Equal("文档", span.Text);
        Assert.Equal("https://example.test/a_(b)", span.NavigateUri!.OriginalString);
    }

    [Fact]
    public void AdjacentLinksDoNotConsumePunctuationOrOtherInlineStyles()
    {
        var spans = MarkdownInlineParser.Parse("打开 [一](https://example.test/a_(b))，再看 [二](mailto:test@example.test)。 **重点** 与 `代码` 和 _说明_").ToArray();
        Assert.Equal("打开 一，再看 二。 重点 与 代码 和 说明", string.Concat(spans.Select(span => span.Text)));
        Assert.Equal(["https://example.test/a_(b)", "mailto:test@example.test"],
            spans.Where(span => span.Kind == MarkdownInlineKind.Link).Select(span => span.NavigateUri!.OriginalString));
        Assert.Equal("重点", Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Bold).Text);
        Assert.Equal("代码", Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Code).Text);
        Assert.Equal("说明", Assert.Single(spans, span => span.Kind == MarkdownInlineKind.Italic).Text);
    }

    [Theory]
    [InlineData("[array[index]](https://example.test)", "array[index]")]
    [InlineData("[右\\]括号](https://example.test)", "右]括号")]
    public void LabelsKeepNestedAndEscapedBrackets(string markdown, string expected)
    {
        var span = Assert.Single(MarkdownInlineParser.Parse(markdown));
        Assert.Equal(MarkdownInlineKind.Link, span.Kind);
        Assert.Equal(expected, span.Text);
    }

    [Theory]
    [InlineData("javascript:alert(1)")]
    [InlineData("data:text/html,hello")]
    [InlineData("file:///C:/test.txt")]
    [InlineData("ms-settings:display")]
    [InlineData("/relative/path")]
    [InlineData("#section")]
    public void UnsupportedDestinationsRemainCompleteLiteralText(string destination)
    {
        var markdown = $"[文字]({destination})";
        var span = Assert.Single(MarkdownInlineParser.Parse(markdown));
        Assert.Equal(MarkdownInlineKind.Text, span.Kind);
        Assert.Equal(markdown, span.Text);
        Assert.Null(span.NavigateUri);
    }

    [Theory]
    [InlineData("[文档](https://example.test/a_(b)")]
    [InlineData("[文档](https://example.test/a_(b) \"unfinished)")]
    [InlineData("[文档](<https://example.test/a_(b))")]
    [InlineData("[文档](https://example.test/a_(b) unquoted title)")]
    [InlineData("[文档](https://example.test/a (nested (title)))")]
    public void IncompleteStreamingOrMalformedLinksKeepAllSourceText(string markdown)
    {
        var spans = MarkdownInlineParser.Parse(markdown).ToArray();
        Assert.DoesNotContain(spans, span => span.Kind == MarkdownInlineKind.Link);
        Assert.Equal(markdown, string.Concat(spans.Select(span => span.Text)));
    }

    [Fact]
    public void InlineCodeKeepsLinkSyntaxAndEscapesLiteral()
    {
        const string source = "[文字](https://example.test/a_(b)) **literal** \\[x]";
        var span = Assert.Single(MarkdownInlineParser.Parse($"`{source}`"));
        Assert.Equal(MarkdownInlineKind.Code, span.Kind);
        Assert.Equal(source, span.Text);
        Assert.Null(span.NavigateUri);
    }

    [Fact]
    public void EscapedOpeningBracketDoesNotBecomeALink()
    {
        var spans = MarkdownInlineParser.Parse("\\[文字](https://example.test/a_(b))").ToArray();
        Assert.DoesNotContain(spans, span => span.Kind == MarkdownInlineKind.Link);
        Assert.Equal("[文字](https://example.test/a_(b))", string.Concat(spans.Select(span => span.Text)));
    }

    [Theory]
    [InlineData(null)]
    [InlineData("")]
    [InlineData("  普通文字\t ")]
    public void PlainTextAndEmptyInputKeepTheirContent(string? source)
    {
        var spans = MarkdownInlineParser.Parse(source).ToArray();
        Assert.All(spans, span => Assert.Equal(MarkdownInlineKind.Text, span.Kind));
        Assert.Equal(source ?? string.Empty, string.Concat(spans.Select(span => span.Text)));
    }
}
