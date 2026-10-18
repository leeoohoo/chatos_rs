using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace ChatOS.Desktop.DesignSystem;

public sealed class MarkdownRichTextBlock : UserControl
{
    private readonly RichTextBlock _target = new() { IsTextSelectionEnabled = true };

    public static readonly DependencyProperty MarkdownProperty = DependencyProperty.Register(
        nameof(Markdown),
        typeof(string),
        typeof(MarkdownRichTextBlock),
        new PropertyMetadata(string.Empty, OnMarkdownChanged));

    public string Markdown
    {
        get => (string)GetValue(MarkdownProperty);
        set => SetValue(MarkdownProperty, value);
    }

    public MarkdownRichTextBlock()
    {
        Content = _target;
    }

    private static void OnMarkdownChanged(
        DependencyObject dependencyObject,
        DependencyPropertyChangedEventArgs args)
    {
        if (dependencyObject is MarkdownRichTextBlock target)
        {
            MarkdownRichTextRenderer.Render(target._target, args.NewValue as string);
        }
    }
}
