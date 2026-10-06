using Microsoft.UI.Xaml.Media.Imaging;
using Windows.Storage;

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
            var framesDirectory = Path.Combine(
                AppContext.BaseDirectory,
                "Assets",
                "Pets",
                "fengtuan",
                "frames");
            if (!Directory.Exists(framesDirectory)) return null;

            var frames = new List<BitmapImage>(PetSpriteAtlasPolicy.FrameCount);
            for (var row = 0; row < PetSpriteAtlasPolicy.Rows; row++)
            {
                for (var column = 0; column < PetSpriteAtlasPolicy.Columns; column++)
                {
                    var path = Path.Combine(
                        framesDirectory,
                        PetSpriteAtlasPolicy.FrameFileName(row, column));
                    var info = new FileInfo(path);
                    if (!info.Exists || info.Length is <= 0 or > PetSpriteAtlasPolicy.MaximumFrameBytes)
                        return null;

                    var file = await StorageFile.GetFileFromPathAsync(path);
                    using var frameStream = await file.OpenReadAsync();
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
