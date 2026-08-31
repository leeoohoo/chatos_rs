namespace VisualComputerUse.Windows;

internal static class DisplayService
{
    internal static IReadOnlyList<DisplayDto> GetDisplays()
    {
        var displays = new List<DisplayDto>();
        var callback = new NativeMethods.MonitorEnumProc((
            nint monitor,
            nint hdc,
            ref NativeMethods.Rect monitorRect,
            nint data) =>
        {
            var info = new NativeMethods.MonitorInfoEx
            {
                Size = System.Runtime.InteropServices.Marshal.SizeOf<NativeMethods.MonitorInfoEx>(),
                DeviceName = string.Empty
            };
            if (!NativeMethods.GetMonitorInfo(monitor, ref info))
                return true;

            var width = info.Monitor.Right - info.Monitor.Left;
            var height = info.Monitor.Bottom - info.Monitor.Top;
            displays.Add(new DisplayDto(
                Id: monitor.ToInt64().ToString("X"),
                IsPrimary: (info.Flags & NativeMethods.MonitorInfofPrimary) != 0,
                DeviceName: info.DeviceName,
                Frame: new RectDto(info.Monitor.Left, info.Monitor.Top, width, height),
                NativePixelWidth: width,
                NativePixelHeight: height,
                NativePixelsPerPointX: 1,
                NativePixelsPerPointY: 1));
            return true;
        });

        if (!NativeMethods.EnumDisplayMonitors(0, 0, callback, 0) || displays.Count == 0)
            throw new VisualComputerUseException("Could not enumerate active Windows displays.");
        return displays;
    }

    internal static RectDto DesktopBounds(IReadOnlyList<DisplayDto> displays)
    {
        if (displays.Count == 0)
            return new RectDto(0, 0, 0, 0);
        var minX = displays.Min(display => display.Frame.X);
        var minY = displays.Min(display => display.Frame.Y);
        var maxX = displays.Max(display => display.Frame.X + display.Frame.Width);
        var maxY = displays.Max(display => display.Frame.Y + display.Frame.Height);
        return new RectDto(minX, minY, maxX - minX, maxY - minY);
    }
}
