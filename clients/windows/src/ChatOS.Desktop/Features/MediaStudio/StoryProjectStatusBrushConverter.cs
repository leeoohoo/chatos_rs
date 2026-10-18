using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media;

namespace ChatOS.Desktop.Features.MediaStudio;

public sealed class StoryProjectStatusBrushConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language)
    {
        var tone = value is StoryProjectStatusTone status
            ? status
            : StoryProjectStatusTone.Production;
        var role = parameter as string;
        var color = (tone, role) switch
        {
            (StoryProjectStatusTone.Draft, "Background") => (0xFE, 0xF3, 0xC7),
            (StoryProjectStatusTone.Draft, "Border") => (0xFD, 0xBA, 0x74),
            (StoryProjectStatusTone.Draft, _) => (0xD9, 0x77, 0x06),
            (StoryProjectStatusTone.Complete, "Background") => (0xDC, 0xFC, 0xE7),
            (StoryProjectStatusTone.Complete, "Border") => (0x86, 0xEF, 0xAC),
            (StoryProjectStatusTone.Complete, _) => (0x16, 0x80, 0x3B),
            (StoryProjectStatusTone.Review, "Background") => (0xFE, 0xE2, 0xE2),
            (StoryProjectStatusTone.Review, "Border") => (0xFC, 0xA5, 0xA5),
            (StoryProjectStatusTone.Review, _) => (0xDC, 0x26, 0x26),
            (StoryProjectStatusTone.Production, "Background") => (0xEE, 0xEE, 0xFF),
            (StoryProjectStatusTone.Production, "Border") => (0xC7, 0xD2, 0xFE),
            _ => (0x5B, 0x5B, 0xD6),
        };
        return new SolidColorBrush(Windows.UI.Color.FromArgb(
            0xFF,
            (byte)color.Item1,
            (byte)color.Item2,
            (byte)color.Item3));
    }

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}
