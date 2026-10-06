using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Graphics.Imaging;
using Windows.Storage;
using Windows.Storage.Streams;

namespace ChatOS.Desktop.Features.Pet;

internal static class PetSpriteAtlas
{
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
            if (!info.Exists || info.Length is <= 0 or > PetSpriteAtlasPolicy.MaximumBytes)
                return null;

            var file = await StorageFile.GetFileFromPathAsync(path);
            using var input = await file.OpenReadAsync();
            var decoder = await BitmapDecoder.CreateAsync(input);
            if (!PetSpriteAtlasPolicy.HasExpectedDimensions(
                    checked((int)decoder.PixelWidth),
                    checked((int)decoder.PixelHeight)))
            {
                return null;
            }

            var frames = new List<BitmapImage>(PetSpriteAtlasPolicy.FrameCount);
            for (var row = 0; row < PetSpriteAtlasPolicy.Rows; row++)
            {
                for (var column = 0; column < PetSpriteAtlasPolicy.Columns; column++)
                {
                    var transform = new BitmapTransform
                    {
                        Bounds = new BitmapBounds
                        {
                            X = (uint)(column * PetSpriteAtlasPolicy.CellWidth),
                            Y = (uint)(row * PetSpriteAtlasPolicy.CellHeight),
                            Width = PetSpriteAtlasPolicy.CellWidth,
                            Height = PetSpriteAtlasPolicy.CellHeight,
                        },
                        ScaledWidth = PetSpriteAtlasPolicy.RenderWidth,
                        ScaledHeight = PetSpriteAtlasPolicy.RenderHeight,
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
                        PetSpriteAtlasPolicy.RenderWidth,
                        PetSpriteAtlasPolicy.RenderHeight,
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
            return frames.Count == PetSpriteAtlasPolicy.FrameCount ? frames : null;
        }
        catch
        {
            return null;
        }
    }
}
