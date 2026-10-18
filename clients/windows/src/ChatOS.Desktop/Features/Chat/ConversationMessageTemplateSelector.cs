using ChatOS.Presentation.Chat;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace ChatOS.Desktop.Features.Chat;

public sealed class ConversationMessageTemplateSelector : DataTemplateSelector
{
    public DataTemplate UserTemplate { get; set; } = null!;

    public DataTemplate ReplyTemplate { get; set; } = null!;

    protected override DataTemplate SelectTemplateCore(object item) =>
        item is ConversationUserMessageItemViewModel ? UserTemplate : ReplyTemplate;

    protected override DataTemplate SelectTemplateCore(object item, DependencyObject container) =>
        SelectTemplateCore(item);
}
