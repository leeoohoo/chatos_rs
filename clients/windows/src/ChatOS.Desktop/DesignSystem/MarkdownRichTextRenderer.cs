using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Documents;
using Microsoft.UI.Xaml.Markup;
using Microsoft.UI.Xaml.Media;
using System.Text.RegularExpressions;
using ChatOS.Presentation.Markdown;

namespace ChatOS.Desktop.DesignSystem;

internal static partial class MarkdownRichTextRenderer
{
    public static void Render(RichTextBlock target, string? markdown)
    {
        target.Blocks.Clear();
        foreach (var sourceBlock in MarkdownLineParser.ParseDocumentBlocks(markdown))
        {
            var paragraph = new Paragraph { Margin = new Thickness(0, 0, 0, 8) };
            if (sourceBlock.IsCode)
            {
                if (sourceBlock.LanguageLabel is { } languageLabel)
                {
                    // ThemeResource keeps the native secondary text color in
                    // sync when the same reader switches between light and dark.
                    var label = (Paragraph)XamlReader.Load("""
                        <Paragraph xmlns="http://schemas.microsoft.com/winfx/2006/xaml/presentation"
                            FontSize="10" FontWeight="SemiBold" Margin="0,0,0,4"
                            Foreground="{ThemeResource TextFillColorSecondaryBrush}" />
                        """);
                    label.Inlines.Add(new Run { Text = languageLabel });
                    target.Blocks.Add(label);
                }
                paragraph.FontFamily = new FontFamily("Cascadia Mono, Consolas");
                paragraph.FontSize = 12;
                paragraph.LineHeight = 19;
                // Keep code in one native paragraph. Each source newline is a
                // LineBreak, without per-line paragraph spacing or inline markup.
                for (var index = 0; index < sourceBlock.Lines.Count; index++)
                {
                    if (index > 0) paragraph.Inlines.Add(new LineBreak());
                    paragraph.Inlines.Add(new Run { Text = sourceBlock.Lines[index] });
                }
                target.Blocks.Add(paragraph);
                continue;
            }

            var line = sourceBlock.Lines[0];
            if (sourceBlock.IsProse)
            {
                // Joining soft source lines must not manufacture a heading or
                // list marker that did not exist on any original source line.
                AppendInlineMarkdown(paragraph, line);
                target.Blocks.Add(paragraph);
                continue;
            }
            var text = line;
            if (MarkdownHeadingParser.Parse(line) is { } heading)
            {
                // Keep the existing Windows size hierarchy; lower headings use
                // native body size with emphasis, as on the Mac reader.
                paragraph.FontSize = heading.Level switch { 1 => 24, 2 => 19, 3 => 16, _ => 14 };
                paragraph.FontWeight = heading.Level == 1 ? FontWeights.Bold : FontWeights.SemiBold;
                text = heading.Text;
            }
            else if (line.TrimStart().StartsWith("- ", StringComparison.Ordinal) ||
                     line.TrimStart().StartsWith("* ", StringComparison.Ordinal) ||
                     line.TrimStart().StartsWith("+ ", StringComparison.Ordinal))
            {
                text = "• " + line.TrimStart()[2..];
            }
            else if (OrderedListPattern().Match(line) is { Success: true } ordered)
            {
                text = $"{ordered.Groups["number"].Value}. {ordered.Groups["item"].Value}";
            }
            else if (HorizontalRulePattern().IsMatch(line))
            {
                text = "────────────────────────";
                paragraph.Foreground = new SolidColorBrush(Microsoft.UI.Colors.Gray);
            }
            else if (line.TrimStart().StartsWith("> ", StringComparison.Ordinal))
            {
                paragraph.FontStyle = Windows.UI.Text.FontStyle.Italic;
                text = line.TrimStart()[2..];
            }

            AppendInlineMarkdown(paragraph, text);
            target.Blocks.Add(paragraph);
        }
    }

    private static void AppendInlineMarkdown(Paragraph paragraph, string text)
    {
        foreach (var span in MarkdownInlineParser.Parse(text))
        {
            if (span.Kind == MarkdownInlineKind.Bold)
            {
                paragraph.Inlines.Add(new Run
                {
                    Text = span.Text,
                    FontWeight = FontWeights.SemiBold,
                });
            }
            else if (span.Kind == MarkdownInlineKind.Code)
            {
                paragraph.Inlines.Add(new Run
                {
                    Text = span.Text,
                    FontFamily = new FontFamily("Cascadia Mono, Consolas"),
                    FontSize = 12,
                });
            }
            else if (span.Kind == MarkdownInlineKind.Italic)
            {
                paragraph.Inlines.Add(new Run
                {
                    Text = span.Text,
                    FontStyle = Windows.UI.Text.FontStyle.Italic,
                });
            }
            else if (span.Kind == MarkdownInlineKind.Strikethrough)
            {
                paragraph.Inlines.Add(new Run
                {
                    Text = span.Text,
                    TextDecorations = Windows.UI.Text.TextDecorations.Strikethrough,
                });
            }
            else if (span.Kind == MarkdownInlineKind.Link && span.NavigateUri is { } uri)
            {
                var hyperlink = new Hyperlink { NavigateUri = uri };
                hyperlink.Inlines.Add(new Run { Text = span.Text });
                paragraph.Inlines.Add(hyperlink);
            }
            else paragraph.Inlines.Add(new Run { Text = span.Text });
        }
    }

    [GeneratedRegex(@"^\s*(?<number>\d+)[.)]\s+(?<item>.+)$")]
    private static partial Regex OrderedListPattern();

    [GeneratedRegex(@"^\s*(?:-{3,}|\*{3,}|_{3,})\s*$")]
    private static partial Regex HorizontalRulePattern();
}
