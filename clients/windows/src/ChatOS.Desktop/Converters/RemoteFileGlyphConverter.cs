using Microsoft.UI.Xaml.Data;

namespace ChatOS.Desktop.Converters;

public sealed class RemoteFileGlyphConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) => value is true ? "\uE8B7" : "\uE7C3";
    public object ConvertBack(object value, Type targetType, object parameter, string language) => throw new NotSupportedException();
}
