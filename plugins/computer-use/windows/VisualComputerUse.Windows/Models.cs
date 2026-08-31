namespace VisualComputerUse.Windows;

internal sealed record PointDto(double X, double Y);

internal sealed record RectDto(double X, double Y, double Width, double Height)
{
    public bool Contains(PointDto point) =>
        point.X >= X && point.X < X + Width &&
        point.Y >= Y && point.Y < Y + Height;

    public bool Contains(RectDto rect) =>
        rect.Width > 0 && rect.Height > 0 &&
        rect.X >= X && rect.Y >= Y &&
        rect.X + rect.Width <= X + Width &&
        rect.Y + rect.Height <= Y + Height;
}

internal sealed record DisplayDto(
    string Id,
    bool IsPrimary,
    string DeviceName,
    RectDto Frame,
    int NativePixelWidth,
    int NativePixelHeight,
    double NativePixelsPerPointX,
    double NativePixelsPerPointY);

internal sealed record ObservationDto(
    string CoordinateSystem,
    ActiveApplicationDto ActiveApplication,
    RectDto GlobalDesktopBounds,
    DisplayDto SelectedDisplay,
    IReadOnlyList<DisplayDto> Displays,
    RectDto CaptureRegionGlobal,
    PointDto VirtualCursorGlobal,
    PointDto? CursorScreenshotPixel,
    bool VirtualCursorIsInCaptureRegion,
    string CursorVisualization,
    int ScreenshotPixelWidth,
    int ScreenshotPixelHeight,
    double GlobalPointsPerScreenshotPixelX,
    double GlobalPointsPerScreenshotPixelY,
    string ImageFormat,
    int EncodedByteCount,
    bool CursorMarkerIncluded,
    string CapturedAt);

internal sealed record CapturedObservation(
    ObservationDto Metadata,
    byte[] ImageData,
    string MimeType);

internal sealed record PermissionItemDto(
    string Kind,
    string Title,
    bool Granted,
    string Purpose,
    string NextStep);

internal sealed record PermissionDto(
    bool ScreenCaptureAvailable,
    bool InputDesktopAvailable,
    bool ProcessElevated,
    bool AllGranted,
    bool OnboardingPresented,
    string ApplicationName,
    string Executable,
    IReadOnlyList<string> Limitations,
    IReadOnlyList<PermissionItemDto> Permissions,
    IReadOnlyList<string> Guidance);

internal sealed record ActiveApplicationDto(
    string? Name,
    string? Executable,
    int? ProcessIdentifier,
    string? WindowTitle);

internal sealed record ShortcutDefinition(
    string Id,
    string Title,
    IReadOnlyList<string> Keys,
    string? Description);

internal sealed record ShortcutListDto(
    ActiveApplicationDto Application,
    IReadOnlyList<ShortcutDefinition> Shortcuts,
    string Source);

internal enum ScreenshotFormat
{
    Png,
    Jpeg
}

internal sealed record ObservationOptions(
    string? DisplayId,
    RectDto? Region,
    int MaxImageWidth,
    ScreenshotFormat Format,
    double JpegQuality);

internal sealed class VisualComputerUseException(string message) : Exception(message);
