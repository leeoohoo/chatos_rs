using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Graphics.Imaging;
using Windows.Storage;
using Windows.Storage.Streams;

namespace ChatOS.Desktop.Features.Pet;

internal static class PetSpriteAtlas
{
    public const int Columns = 8;
    public const int Rows = 11;
    private const int CellWidth = 192;
    private const int CellHeight = 208;
    private const int RenderWidth = 180;
    private const int RenderHeight = 195;
    private const long MaximumBytes = 20L * 1024 * 1024;
    private static readonly object Sync = new();
    private static Task<IReadOnlyList<BitmapImage>?>? _loadingTask;

    public static async Task<IReadOnlyList<BitmapImage>?> LoadAsync(
        CancellationToken cancellationToken = default)
    {
        Task<IReadOnlyList<BitmapImage>?> task;
        lock (Sync)
        {
            task = _loadingTask ??= LoadCoreAsync();
        }
        return await task.WaitAsync(cancellationToken);
    }

    private static async Task<IReadOnlyList<BitmapImage>?> LoadCoreAsync()
    {
        try
        {
            var path = Path.Combine(
                AppContext.BaseDirectory,
                "Assets",
                "Pets",
                "fengtuan",
                "spritesheet.webp");
            var info = new FileInfo(path);
            if (!info.Exists || info.Length is <= 0 or > MaximumBytes) return null;

            var file = await StorageFile.GetFileFromPathAsync(path);
            using var input = await file.OpenReadAsync();
            var decoder = await BitmapDecoder.CreateAsync(input);
            if (decoder.PixelWidth != Columns * CellWidth ||
                decoder.PixelHeight != Rows * CellHeight)
            {
                return null;
            }

            var frames = new List<BitmapImage>(Columns * Rows);
            for (var row = 0; row < Rows; row++)
            {
                for (var column = 0; column < Columns; column++)
                {
                    var transform = new BitmapTransform
                    {
                        Bounds = new BitmapBounds
                        {
                            X = (uint)(column * CellWidth),
                            Y = (uint)(row * CellHeight),
                            Width = CellWidth,
                            Height = CellHeight,
                        },
                        ScaledWidth = RenderWidth,
                        ScaledHeight = RenderHeight,
                        InterpolationMode = BitmapInterpolationMode.Fant,
                    };
                    var pixels = await decoder.GetPixelDataAsync(
                        BitmapPixelFormat.Bgra8,
                        BitmapAlphaMode.Premultiplied,
                        transform,
                        ExifOrientationMode.IgnoreExifOrientation,
                        ColorManagementMode.DoNotColorManage);
                    using var frameStream = new InMemoryRandomAccessStream();
                    var encoder = await BitmapEncoder.CreateAsync(
                        BitmapEncoder.PngEncoderId,
                        frameStream);
                    encoder.SetPixelData(
                        BitmapPixelFormat.Bgra8,
                        BitmapAlphaMode.Premultiplied,
                        RenderWidth,
                        RenderHeight,
                        96,
                        96,
                        pixels.DetachPixelData());
                    await encoder.FlushAsync();
                    frameStream.Seek(0);
                    var frame = new BitmapImage();
                    await frame.SetSourceAsync(frameStream);
                    frames.Add(frame);
                }
            }
            return frames.Count == Columns * Rows ? frames : null;
        }
        catch
        {
            return null;
        }
    }
}

internal readonly record struct PetSpriteSequence(
    int Row,
    int FrameCount,
    TimeSpan FrameDuration)
{
    public static PetSpriteSequence For(
        ChatOS.Core.Domain.PetAnimationState state,
        bool isDragging,
        bool isDraggingRight) =>
        isDragging
            ? new PetSpriteSequence(isDraggingRight ? 1 : 2, 8, TimeSpan.FromMilliseconds(100))
            : state switch
            {
                ChatOS.Core.Domain.PetAnimationState.Succeeded =>
                    new PetSpriteSequence(4, 5, TimeSpan.FromMilliseconds(500)),
                ChatOS.Core.Domain.PetAnimationState.Failed =>
                    new PetSpriteSequence(5, 8, TimeSpan.FromMilliseconds(500)),
                ChatOS.Core.Domain.PetAnimationState.Waiting =>
                    new PetSpriteSequence(6, 6, TimeSpan.FromMilliseconds(500)),
                ChatOS.Core.Domain.PetAnimationState.Running =>
                    new PetSpriteSequence(7, 6, TimeSpan.FromMilliseconds(200)),
                ChatOS.Core.Domain.PetAnimationState.Review =>
                    new PetSpriteSequence(8, 6, TimeSpan.FromMilliseconds(250)),
                _ => new PetSpriteSequence(0, 7, TimeSpan.FromSeconds(1)),
            };
}
