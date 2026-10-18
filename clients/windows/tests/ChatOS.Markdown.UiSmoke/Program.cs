using ChatOS.Desktop.DesignSystem;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Media.Imaging;
using System.Text.Json;
using Windows.Graphics;
using Windows.Graphics.Imaging;
using Windows.Security.Cryptography;
using Windows.Storage;

namespace ChatOS.Markdown.UiSmoke;

internal static class Program
{
    [STAThread]
    private static void Main()
    {
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(initializationParams =>
        {
            SynchronizationContext.SetSynchronizationContext(
                new DispatcherQueueSynchronizationContext(DispatcherQueue.GetForCurrentThread()));
            _ = new MarkdownSmokeApp();
        });
    }
}

public sealed partial class MarkdownSmokeApp : Application
{
    private Window? _window;
    private readonly string _outputDirectory = Path.GetFullPath(Environment.GetCommandLineArgs()[1]);

    public MarkdownSmokeApp()
    {
        UnhandledException += (_, exception) =>
        {
            Directory.CreateDirectory(_outputDirectory);
            File.WriteAllText(Path.Combine(_outputDirectory, "unhandled.txt"), exception.Exception.ToString());
        };
        InitializeComponent();
    }

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        Directory.CreateDirectory(_outputDirectory);
        try
        {
            var passed = VerifyNativeRendering();
            var reader = new RichTextBlock { IsTextSelectionEnabled = true };
            MarkdownRichTextRenderer.Render(reader, """
                # Native Markdown sample
                Language labels, literal code and complete link destinations.

                ~~旧版本 Old version~~ becomes current. `~~literal code~~` and \~~escaped~~ stay literal.

                Soft source lines form one paragraph, including ``one`
                two`` across the line boundary.

                ```csharp
                    var _name_ = "**literal**";

                    return _name_;
                ```
                ```json
                { "status": "ready", "中文": "内容" }
                ```
                ```
                unlabelled code stays unlabelled
                ```
                Open [Function](https://example.test/Function_(math)) or [Document](<https://example.test/a_(b)> "Title").

                **Bold** and _italic_ with `literal [link](https://example.test)`.

                Use `` `tick` **literal** `` beside `single`.

                ## Heading 2
                ### Heading 3
                #### Heading 4 **bold**
                ##### Heading 5 `code`
                ###### Heading 6 [link](https://example.test)
                """);
            var root = new Border
            {
                Padding = new Thickness(20),
                RequestedTheme = ElementTheme.Light,
                Background = new Microsoft.UI.Xaml.Media.SolidColorBrush(Microsoft.UI.Colors.White),
                Child = new ScrollViewer { Content = reader },
            };
            _window = new Window { Title = "ChatOS Markdown local UI smoke", Content = root };
            _window.AppWindow.Resize(new SizeInt32(760, 760));
            _window.Activate();
            await Task.Delay(400);
            await CaptureAsync(root, "light.png");
            root.RequestedTheme = ElementTheme.Dark;
            root.Background = new Microsoft.UI.Xaml.Media.SolidColorBrush(Microsoft.UI.Colors.Black);
            await Task.Delay(250);
            await CaptureAsync(root, "dark.png");
            _window.AppWindow.Resize(new SizeInt32(360, 760));
            await Task.Delay(250);
            await CaptureAsync(root, "dark-narrow.png");
            await File.WriteAllTextAsync(Path.Combine(_outputDirectory, "result.json"), JsonSerializer.Serialize(new
            {
                passed,
                runId = Environment.GetCommandLineArgs()[2],
                failed = 0,
                skipped = 0,
                scope = "Isolated native renderer; authenticated pages, clipboard and browser activation are not tested.",
                screenshots = new[] { "light.png", "dark.png", "dark-narrow.png" },
            }, new JsonSerializerOptions { WriteIndented = true }));
        }
        catch (Exception exception)
        {
            Environment.ExitCode = 1;
            await File.WriteAllTextAsync(Path.Combine(_outputDirectory, "failure.txt"), exception.ToString());
        }
        finally
        {
            _window?.Close();
            Exit();
        }
    }

    private static int VerifyNativeRendering()
    {
        var target = new RichTextBlock { IsTextSelectionEnabled = true };
        MarkdownRichTextRenderer.Render(target, "```csharp\n    one\n\n    two\n```");
        Require(target.Blocks.Count == 2 && ParagraphText(target, 0) == "CSHARP", "Language label is missing.");
        var code = (Paragraph)target.Blocks[1];
        Require(code.Inlines.OfType<LineBreak>().Count() == 2 &&
            code.Inlines.OfType<Run>().Select(run => run.Text).SequenceEqual(new[] { "    one", "", "    two" }),
            "Code body lines or indentation changed.");

        MarkdownRichTextRenderer.Render(target, "~~~\nplain\n~~~");
        Require(target.Blocks.Count == 1 && ParagraphText(target, 0) == "plain", "Unlabelled code gained a label.");
        MarkdownRichTextRenderer.Render(target, "```**json**\n{}");
        Require(ParagraphText(target, 0) == "**JSON**" && ParagraphText(target, 1) == "{}",
            "Streaming language info was interpreted as inline Markdown.");
        MarkdownRichTextRenderer.Render(target, "[Function](https://example.test/Function_(math))");
        var link = ((Paragraph)target.Blocks[0]).Inlines.OfType<Hyperlink>().Single();
        Require(link.NavigateUri.OriginalString == "https://example.test/Function_(math)", "Native link destination was truncated.");
        MarkdownRichTextRenderer.Render(target, "```text\n[not a link](https://example.test) **literal**\n```");
        Require(((Paragraph)target.Blocks[1]).Inlines.All(inline => inline is Run) &&
            ParagraphText(target, 1) == "[not a link](https://example.test) **literal**" && target.IsTextSelectionEnabled,
            "Fenced code lost literal content or native selection.");
        MarkdownRichTextRenderer.Render(target, "Use `` `tick` **literal** [link](https://example.test) `` after.");
        var codeSpan = ((Paragraph)target.Blocks[0]).Inlines.OfType<Run>()
            .Single(run => run.FontFamily?.Source == "Cascadia Mono, Consolas");
        Require(codeSpan.Text == "`tick` **literal** [link](https://example.test)" &&
            !((Paragraph)target.Blocks[0]).Inlines.OfType<Hyperlink>().Any() &&
            ParagraphText(target, 0) == "Use `tick` **literal** [link](https://example.test) after.",
            "Multi-backtick code changed literal content or neighboring text.");
        MarkdownRichTextRenderer.Render(target, "Use ``unfinished`code");
        Require(ParagraphText(target, 0) == "Use ``unfinished`code", "Incomplete code span lost its delimiters.");
        MarkdownRichTextRenderer.Render(target, "Use ``unfinished`code``");
        Require(ParagraphText(target, 0) == "Use unfinished`code" && target.IsTextSelectionEnabled,
            "Completed code span kept stale literal content or lost native selection.");
        MarkdownRichTextRenderer.Render(target, "First source line\nsecond source line\n\nNext paragraph");
        Require(target.Blocks.Count == 2 && ParagraphText(target, 0) == "First source line second source line" &&
            ParagraphText(target, 1) == "Next paragraph", "Soft lines were rendered as separate or empty paragraphs.");
        MarkdownRichTextRenderer.Render(target, "Use ``one`\ntwo`` after.");
        Require(target.Blocks.Count == 1 && ParagraphText(target, 0) == "Use one` two after." &&
            ((Paragraph)target.Blocks[0]).Inlines.OfType<Run>().Single(
                run => run.FontFamily?.Source == "Cascadia Mono, Consolas").Text == "one` two",
            "Code span did not continue across a prose soft line.");
        MarkdownRichTextRenderer.Render(target, "Before\n# Heading\n- Item\n> Quote\n```text\n    one\n\n    two\n```\nAfter");
        Require(target.Blocks.Count == 7 && ParagraphText(target, 0) == "Before" &&
            ParagraphText(target, 1) == "Heading" && ParagraphText(target, 2) == "• Item" &&
            ParagraphText(target, 3) == "Quote" && ParagraphText(target, 4) == "TEXT" &&
            ((Paragraph)target.Blocks[5]).Inlines.OfType<LineBreak>().Count() == 2 &&
            ParagraphText(target, 6) == "After" && target.IsTextSelectionEnabled,
            "Prose grouping consumed a structural block or code whitespace.");
        MarkdownRichTextRenderer.Render(target, string.Join("\n", Enumerable.Range(1, 6)
            .Select(level => $"{new string('#', level)} Heading {level}")));
        Require(target.Blocks.Count == 6 && Enumerable.Range(0, 6).All(index =>
            ParagraphText(target, index) == $"Heading {index + 1}" &&
            ((Paragraph)target.Blocks[index]).FontWeight.Weight >= 600) &&
            ((Paragraph)target.Blocks[0]).FontSize > ((Paragraph)target.Blocks[1]).FontSize &&
            ((Paragraph)target.Blocks[1]).FontSize > ((Paragraph)target.Blocks[2]).FontSize &&
            ((Paragraph)target.Blocks[2]).FontSize >= ((Paragraph)target.Blocks[3]).FontSize,
            "The native reader did not render all six heading levels.");
        MarkdownRichTextRenderer.Render(target, "####\t**Bold** [Document](https://example.test)\n##### \n####### literal\n\\#### escaped");
        var headingLink = ((Paragraph)target.Blocks[0]).Inlines.OfType<Hyperlink>().Single();
        Require(target.Blocks.Count == 2 &&
            ((Paragraph)target.Blocks[0]).Inlines.OfType<Run>().First().Text == "Bold" &&
            headingLink.NavigateUri.AbsoluteUri == "https://example.test/" &&
            ParagraphText(target, 1) == "##### ####### literal #### escaped" &&
            ((Paragraph)target.Blocks[1]).FontWeight.Weight == 400 && target.IsTextSelectionEnabled,
            $"Heading whitespace, inline styles or literal invalid headings changed: blocks={target.Blocks.Count}, " +
            $"firstRun={((Paragraph)target.Blocks[0]).Inlines.OfType<Run>().First().Text}, uri={headingLink.NavigateUri.OriginalString}, " +
            $"prose={ParagraphText(target, 1)}, weight={((Paragraph)target.Blocks[1]).FontWeight.Weight}.");
        MarkdownRichTextRenderer.Render(target, "-\nnext line\n#hashtag");
        Require(target.Blocks.Count == 1 && ParagraphText(target, 0) == "- next line #hashtag" &&
            ((Paragraph)target.Blocks[0]).FontWeight.Weight == 400,
            "Joining prose source lines manufactured a list item.");
        MarkdownRichTextRenderer.Render(target, "Before ~~旧版本~~ after. **bold** _italic_ [link](https://example.test)");
        var strikeRuns = ((Paragraph)target.Blocks[0]).Inlines.OfType<Run>().ToArray();
        Require(strikeRuns.Count(run => run.TextDecorations == Windows.UI.Text.TextDecorations.Strikethrough) == 1 &&
            strikeRuns.Single(run => run.TextDecorations == Windows.UI.Text.TextDecorations.Strikethrough).Text == "旧版本" &&
            strikeRuns.Any(run => run.Text == "bold" && run.FontWeight.Weight >= 600) &&
            strikeRuns.Any(run => run.Text == "italic" && run.FontStyle == Windows.UI.Text.FontStyle.Italic) &&
            ((Paragraph)target.Blocks[0]).Inlines.OfType<Hyperlink>().Single().NavigateUri.AbsoluteUri == "https://example.test/",
            "Native strikethrough or neighboring inline styles are missing.");
        MarkdownRichTextRenderer.Render(target, "`~~code~~` \\~~escaped~~ ~~unclosed");
        Require(ParagraphText(target, 0) == "~~code~~ ~~escaped~~ ~~unclosed" &&
            ((Paragraph)target.Blocks[0]).Inlines.OfType<Run>().All(
                run => run.TextDecorations == Windows.UI.Text.TextDecorations.None),
            "Code, escaped or incomplete strikethrough gained a decoration.");
        MarkdownRichTextRenderer.Render(target, "~~unclosed~~\n\n```text\n~~literal~~\n```");
        Require(target.Blocks.Count == 3 && ParagraphText(target, 0) == "unclosed" &&
            ((Paragraph)target.Blocks[0]).Inlines.OfType<Run>().Single().TextDecorations == Windows.UI.Text.TextDecorations.Strikethrough &&
            ParagraphText(target, 2) == "~~literal~~" &&
            ((Paragraph)target.Blocks[2]).Inlines.OfType<Run>().Single().TextDecorations == Windows.UI.Text.TextDecorations.None &&
            target.IsTextSelectionEnabled,
            "Completed strikethrough kept stale delimiters, decorated fenced code or lost native selection.");
        return 17;
    }

    private static string ParagraphText(RichTextBlock target, int index) =>
        string.Concat(((Paragraph)target.Blocks[index]).Inlines.OfType<Run>().Select(run => run.Text));

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private async Task CaptureAsync(UIElement root, string fileName)
    {
        var bitmap = new RenderTargetBitmap();
        await bitmap.RenderAsync(root);
        var pixels = await bitmap.GetPixelsAsync();
        CryptographicBuffer.CopyToByteArray(pixels, out var bytes);
        var folder = await StorageFolder.GetFolderFromPathAsync(_outputDirectory);
        var file = await folder.CreateFileAsync(fileName, CreationCollisionOption.ReplaceExisting);
        using var stream = await file.OpenAsync(FileAccessMode.ReadWrite);
        var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
        encoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied,
            (uint)bitmap.PixelWidth, (uint)bitmap.PixelHeight, 96, 96, bytes);
        await encoder.FlushAsync();
    }
}

