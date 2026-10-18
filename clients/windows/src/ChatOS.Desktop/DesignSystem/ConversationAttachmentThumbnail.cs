using ChatOS.Core.Domain;
using System.Security.Cryptography;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Storage.Streams;

namespace ChatOS.Desktop.DesignSystem;

public sealed class ConversationAttachmentThumbnail : UserControl
{
    private readonly Image _image = new()
    {
        Stretch = Stretch.UniformToFill,
        Visibility = Visibility.Collapsed,
    };

    private readonly FontIcon _fallback = new()
    {
        FontSize = 15,
        Foreground = (Brush)Application.Current.Resources["ChatOSPurpleBrush"],
    };

    private int _loadGeneration;

    public static readonly DependencyProperty AttachmentProperty = DependencyProperty.Register(
        nameof(Attachment),
        typeof(ConversationAttachmentDraft),
        typeof(ConversationAttachmentThumbnail),
        new PropertyMetadata(null, OnAttachmentChanged));

    public ConversationAttachmentDraft? Attachment
    {
        get => (ConversationAttachmentDraft?)GetValue(AttachmentProperty);
        set => SetValue(AttachmentProperty, value);
    }

    public ConversationAttachmentThumbnail()
    {
        Content = new Border
        {
            Background = (Brush)Application.Current.Resources["ChatOSPurpleSoftBrush"],
            CornerRadius = new CornerRadius(6),
            Child = new Grid
            {
                Children =
                {
                    _fallback,
                    _image,
                },
            },
        };
    }

    internal static async Task<BitmapImage> DecodeImageAsync(byte[] data)
    {
        using var stream = new InMemoryRandomAccessStream();
        using (var writer = new DataWriter(stream))
        {
            writer.WriteBytes(data);
            await writer.StoreAsync();
            writer.DetachStream();
        }

        stream.Seek(0);
        var source = new BitmapImage
        {
            DecodePixelWidth = 2048,
            DecodePixelType = DecodePixelType.Logical,
        };
        await source.SetSourceAsync(stream);
        return source;
    }

    internal static async Task<BitmapImage> DecodeReferenceImageAsync(
        ConversationAttachmentReference attachment)
    {
        if (attachment.Kind != ConversationAttachmentKind.Image ||
            attachment.Size is <= 0 or > 20 * 1024 * 1024 ||
            !Uri.TryCreate(attachment.ViewUrl ?? attachment.Url, UriKind.Absolute, out var uri))
        {
            throw new InvalidOperationException("The image attachment reference is invalid.");
        }

        if (!uri.IsFile)
        {
            if (uri.Scheme != Uri.UriSchemeHttps)
            {
                throw new InvalidOperationException("The image attachment URL is not allowed.");
            }

            return new BitmapImage(uri)
            {
                DecodePixelWidth = 2048,
                DecodePixelType = DecodePixelType.Logical,
            };
        }

        var path = Path.GetFullPath(uri.LocalPath);
        var info = new FileInfo(path);
        if (!info.Exists || info.Length != attachment.Size ||
            (info.Attributes & FileAttributes.ReparsePoint) != 0)
        {
            throw new InvalidOperationException("The local image attachment is unavailable.");
        }

        var data = await File.ReadAllBytesAsync(path);
        if (!string.IsNullOrWhiteSpace(attachment.Sha256) &&
            !string.Equals(
                Convert.ToHexString(SHA256.HashData(data)),
                attachment.Sha256,
                StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException("The local image attachment failed its integrity check.");
        }

        return await DecodeImageAsync(data);
    }

    private static void OnAttachmentChanged(
        DependencyObject dependencyObject,
        DependencyPropertyChangedEventArgs args)
    {
        if (dependencyObject is ConversationAttachmentThumbnail target)
        {
            _ = target.LoadAttachmentAsync(args.NewValue as ConversationAttachmentDraft);
        }
    }

    private async Task LoadAttachmentAsync(ConversationAttachmentDraft? attachment)
    {
        var generation = ++_loadGeneration;
        _image.Source = null;
        _image.Visibility = Visibility.Collapsed;
        _fallback.Visibility = Visibility.Visible;
        _fallback.Glyph = AttachmentGlyph(attachment);
        if (attachment is not { Kind: ConversationAttachmentKind.Image, Data.Length: > 0 })
        {
            return;
        }

        try
        {
            var source = await DecodeImageAsync(attachment.Data);
            if (generation != _loadGeneration)
            {
                return;
            }

            _image.Source = source;
            _image.Visibility = Visibility.Visible;
            _fallback.Visibility = Visibility.Collapsed;
        }
        catch
        {
            // Keep the stable attachment-type glyph when image decoding fails.
        }
    }

    private static string AttachmentGlyph(ConversationAttachmentDraft? attachment)
    {
        if (attachment?.Origin == ConversationAttachmentOrigin.PastedText)
        {
            return "\uE8A5";
        }

        if (attachment?.Kind == ConversationAttachmentKind.Image)
        {
            return "\uEB9F";
        }

        if (attachment?.Kind == ConversationAttachmentKind.Audio)
        {
            return "\uE8D6";
        }

        return "\uE8A5";
    }
}

public sealed class ConversationAttachmentReferenceThumbnail : UserControl
{
    private readonly Image _image = new()
    {
        Stretch = Stretch.UniformToFill,
        Visibility = Visibility.Collapsed,
    };

    private readonly FontIcon _fallback = new()
    {
        Glyph = "\uEB9F",
        FontSize = 22,
        Foreground = (Brush)Application.Current.Resources["ChatOSPurpleBrush"],
    };

    private int _loadGeneration;

    public static readonly DependencyProperty AttachmentProperty = DependencyProperty.Register(
        nameof(Attachment),
        typeof(ConversationAttachmentReference),
        typeof(ConversationAttachmentReferenceThumbnail),
        new PropertyMetadata(null, OnAttachmentChanged));

    public ConversationAttachmentReference? Attachment
    {
        get => (ConversationAttachmentReference?)GetValue(AttachmentProperty);
        set => SetValue(AttachmentProperty, value);
    }

    public ConversationAttachmentReferenceThumbnail()
    {
        Content = new Border
        {
            Background = (Brush)Application.Current.Resources["ChatOSPurpleSoftBrush"],
            CornerRadius = new CornerRadius(10),
            Child = new Grid
            {
                Children =
                {
                    _fallback,
                    _image,
                },
            },
        };
    }

    private static void OnAttachmentChanged(
        DependencyObject dependencyObject,
        DependencyPropertyChangedEventArgs args)
    {
        if (dependencyObject is ConversationAttachmentReferenceThumbnail target)
        {
            _ = target.LoadAttachmentAsync(args.NewValue as ConversationAttachmentReference);
        }
    }

    private async Task LoadAttachmentAsync(ConversationAttachmentReference? attachment)
    {
        var generation = ++_loadGeneration;
        _image.Source = null;
        _image.Visibility = Visibility.Collapsed;
        _fallback.Visibility = Visibility.Visible;
        if (attachment is null)
        {
            return;
        }

        try
        {
            var source = await ConversationAttachmentThumbnail.DecodeReferenceImageAsync(attachment);
            if (generation != _loadGeneration)
            {
                return;
            }

            _image.Source = source;
            _image.Visibility = Visibility.Visible;
            _fallback.Visibility = Visibility.Collapsed;
        }
        catch
        {
            // The placeholder remains visible for missing, corrupt, or unsupported images.
        }
    }
}
