using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Data;

namespace ChatOS.Desktop.Converters;

public sealed class AgentMessageAlignmentConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) =>
        value is true ? HorizontalAlignment.Right : HorizontalAlignment.Left;

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}

public sealed class AgentMessageBubbleBrushConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        var resourceKey = value is true ? "ChatOSUserBubbleBrush" : "ChatOSSurfaceSubtleBrush";
        return Application.Current.Resources[resourceKey];
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}
