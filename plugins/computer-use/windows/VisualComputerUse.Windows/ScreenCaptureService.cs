using System.IO;
using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;

namespace VisualComputerUse.Windows;

internal sealed class ScreenCaptureService(Dispatcher dispatcher)
{
    internal Task<CapturedObservation> CaptureAsync(
        ObservationOptions options,
        PointDto cursor,
        IReadOnlyList<PointDto> trail,
        IReadOnlyList<DisplayDto> displays,
        ActiveApplicationDto activeApplication) =>
        dispatcher.InvokeAsync(() => Capture(
            options,
            cursor,
            trail,
            displays,
            activeApplication)).Task;

    private static CapturedObservation Capture(
        ObservationOptions options,
        PointDto cursor,
        IReadOnlyList<PointDto> trail,
        IReadOnlyList<DisplayDto> displays,
        ActiveApplicationDto activeApplication)
    {
        var selected = SelectDisplay(options, cursor, displays);
        var region = options.Region ?? selected.Frame;
        if (!selected.Frame.Contains(region))
            throw new VisualComputerUseException("region must be fully contained within one active display.");

        var sourceWidth = checked((int)Math.Round(region.Width));
        var sourceHeight = checked((int)Math.Round(region.Height));
        if (sourceWidth <= 0 || sourceHeight <= 0)
            throw new VisualComputerUseException("region width and height must be greater than zero.");

        var targetWidth = options.MaxImageWidth > 0
            ? Math.Min(options.MaxImageWidth, sourceWidth)
            : sourceWidth;
        var targetHeight = Math.Max(1, (int)Math.Round(region.Height * targetWidth / region.Width));
        var source = CaptureBitmap((int)Math.Round(region.X), (int)Math.Round(region.Y), sourceWidth, sourceHeight);
        var rendered = Render(source, targetWidth, targetHeight, cursor, trail, region);
        var encoded = Encode(rendered, options.Format, options.JpegQuality);
        var cursorPixel = region.Contains(cursor)
            ? new PointDto(
                (cursor.X - region.X) * targetWidth / region.Width,
                (cursor.Y - region.Y) * targetHeight / region.Height)
            : null;

        var metadata = new ObservationDto(
            CoordinateSystem: "Global Windows desktop pixels and screenshot pixels use a top-left origin; x grows right and y grows down. Global coordinates may be negative on displays left of or above the primary display. globalX = captureRegionGlobal.x + imageX * globalPointsPerScreenshotPixelX and globalY = captureRegionGlobal.y + imageY * globalPointsPerScreenshotPixelY. click uses virtualCursorGlobal.",
            ActiveApplication: activeApplication,
            GlobalDesktopBounds: DisplayService.DesktopBounds(displays),
            SelectedDisplay: selected,
            Displays: displays,
            CaptureRegionGlobal: region,
            VirtualCursorGlobal: cursor,
            CursorScreenshotPixel: cursorPixel,
            VirtualCursorIsInCaptureRegion: region.Contains(cursor),
            CursorVisualization: region.Contains(cursor) ? "ai-orbit-reticle-with-cyan-hotspot" : "offscreen-edge-indicator",
            ScreenshotPixelWidth: targetWidth,
            ScreenshotPixelHeight: targetHeight,
            GlobalPointsPerScreenshotPixelX: region.Width / targetWidth,
            GlobalPointsPerScreenshotPixelY: region.Height / targetHeight,
            ImageFormat: options.Format == ScreenshotFormat.Png ? "png" : "jpeg",
            EncodedByteCount: encoded.Length,
            CursorMarkerIncluded: true,
            CapturedAt: DateTimeOffset.UtcNow.ToString("O"));

        return new CapturedObservation(
            metadata,
            encoded,
            options.Format == ScreenshotFormat.Png ? "image/png" : "image/jpeg");
    }

    private static DisplayDto SelectDisplay(
        ObservationOptions options,
        PointDto cursor,
        IReadOnlyList<DisplayDto> displays)
    {
        if (options.DisplayId is not null)
            return displays.FirstOrDefault(display => string.Equals(display.Id, options.DisplayId, StringComparison.OrdinalIgnoreCase))
                ?? throw new VisualComputerUseException($"No active display has id '{options.DisplayId}'.");
        var requestedRegion = options.Region;
        if (requestedRegion is not null)
            return displays.FirstOrDefault(display => display.Frame.Contains(requestedRegion))
                ?? throw new VisualComputerUseException("region must be fully contained within one active display.");
        return displays.FirstOrDefault(display => display.Frame.Contains(cursor))
            ?? displays.FirstOrDefault(display => display.IsPrimary)
            ?? displays.First();
    }

    private static BitmapSource CaptureBitmap(int x, int y, int width, int height)
    {
        var screenDc = NativeMethods.GetDC(0);
        if (screenDc == 0)
            throw new VisualComputerUseException("Could not open the Windows desktop device context.");
        var memoryDc = NativeMethods.CreateCompatibleDC(screenDc);
        if (memoryDc == 0)
        {
            NativeMethods.ReleaseDC(0, screenDc);
            throw new VisualComputerUseException("Could not create the Windows screen capture context.");
        }
        var bitmap = NativeMethods.CreateCompatibleBitmap(screenDc, width, height);
        if (bitmap == 0)
        {
            NativeMethods.DeleteDC(memoryDc);
            NativeMethods.ReleaseDC(0, screenDc);
            throw new VisualComputerUseException("Could not allocate the Windows screen capture surface.");
        }

        var previous = NativeMethods.SelectObject(memoryDc, bitmap);
        try
        {
            if (!NativeMethods.BitBlt(
                    memoryDc, 0, 0, width, height, screenDc, x, y,
                    NativeMethods.SrcCopy | NativeMethods.CaptureBlt))
                throw new VisualComputerUseException("Could not capture the requested Windows screen region.");

            var source = Imaging.CreateBitmapSourceFromHBitmap(
                bitmap,
                0,
                Int32Rect.Empty,
                BitmapSizeOptions.FromEmptyOptions());
            source.Freeze();
            return source;
        }
        finally
        {
            if (previous != 0)
                NativeMethods.SelectObject(memoryDc, previous);
            NativeMethods.DeleteObject(bitmap);
            NativeMethods.DeleteDC(memoryDc);
            NativeMethods.ReleaseDC(0, screenDc);
        }
    }

    private static BitmapSource Render(
        BitmapSource source,
        int width,
        int height,
        PointDto cursor,
        IReadOnlyList<PointDto> trail,
        RectDto region)
    {
        var visual = new DrawingVisual();
        using (var context = visual.RenderOpen())
        {
            context.DrawImage(source, new Rect(0, 0, width, height));
            var scaleX = width / region.Width;
            var scaleY = height / region.Height;
            var visibleTrail = trail
                .Where(region.Contains)
                .Select(point => new Point((point.X - region.X) * scaleX, (point.Y - region.Y) * scaleY))
                .ToArray();
            CursorArtwork.DrawTrail(context, visibleTrail);

            if (region.Contains(cursor))
            {
                CursorArtwork.DrawPointer(
                    context,
                    new Point((cursor.X - region.X) * scaleX, (cursor.Y - region.Y) * scaleY),
                    Math.Max(23, Math.Min(width, height) * 0.032));
            }
            else
            {
                var projectedX = (cursor.X - region.X) * scaleX;
                var projectedY = (cursor.Y - region.Y) * scaleY;
                var margin = Math.Max(24, Math.Min(width, height) * 0.04);
                var indicator = new Point(
                    Math.Clamp(projectedX, margin, width - margin),
                    Math.Clamp(projectedY, margin, height - margin));
                var angle = Math.Atan2(projectedY - indicator.Y, projectedX - indicator.X);
                CursorArtwork.DrawOffscreenIndicator(context, indicator, angle, Math.Max(12, margin * 0.48));
            }
        }

        var rendered = new RenderTargetBitmap(width, height, 96, 96, PixelFormats.Pbgra32);
        rendered.Render(visual);
        rendered.Freeze();
        return rendered;
    }

    private static byte[] Encode(BitmapSource source, ScreenshotFormat format, double jpegQuality)
    {
        BitmapEncoder encoder;
        if (format == ScreenshotFormat.Png)
        {
            encoder = new PngBitmapEncoder();
        }
        else
        {
            encoder = new JpegBitmapEncoder
            {
                QualityLevel = (int)Math.Round(Math.Clamp(jpegQuality, 0.1, 1) * 100)
            };
        }
        encoder.Frames.Add(BitmapFrame.Create(source));
        using var stream = new MemoryStream();
        encoder.Save(stream);
        return stream.ToArray();
    }
}
