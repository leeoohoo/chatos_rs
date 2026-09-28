using Windows.Media.Editing;
using Windows.Storage;

namespace ChatOS.Desktop.Features.MediaStudio;

internal sealed record ExtractedVideoFrame(byte[] Bytes, string MimeType);

internal static class StoryVideoFrameExtractor
{
    private const int MaximumFrameBytes = 20 * 1024 * 1024;

    public static async Task<ExtractedVideoFrame> ExtractLastFrameAsync(
        string videoPath,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var file = await StorageFile.GetFileFromPathAsync(videoPath);
        using var clip = await MediaClip.CreateFromFileAsync(file);
        if (clip.OriginalDuration <= TimeSpan.Zero)
            throw new InvalidDataException("视频时长无效，无法提取成片末帧。");
        var offset = clip.OriginalDuration > TimeSpan.FromMilliseconds(80)
            ? clip.OriginalDuration - TimeSpan.FromMilliseconds(50)
            : TimeSpan.Zero;
        using var thumbnail = await clip.GetThumbnailAsync(
            offset, 0, 0, VideoFramePrecision.NearestFrame);
        await using var input = thumbnail.AsStreamForRead();
        using var output = new MemoryStream();
        await input.CopyToAsync(output, cancellationToken);
        if (output.Length is <= 0 or > MaximumFrameBytes)
            throw new InvalidDataException("提取的成片末帧为空或超过 20 MB。");
        var mimeType = string.Equals(thumbnail.ContentType, "image/png", StringComparison.OrdinalIgnoreCase)
            ? "image/png"
            : "image/jpeg";
        return new ExtractedVideoFrame(output.ToArray(), mimeType);
    }
}
