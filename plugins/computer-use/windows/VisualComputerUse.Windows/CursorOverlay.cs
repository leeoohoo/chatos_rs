using System.Windows;
using System.Windows.Interop;
using System.Windows.Media;
using System.Windows.Threading;

namespace VisualComputerUse.Windows;

internal sealed class CursorOverlayManager(Dispatcher dispatcher)
{
    private readonly Dictionary<string, CursorOverlayWindow> windows = [];
    private string displaySignature = string.Empty;

    internal Task ShowAsync(PointDto cursor, IReadOnlyList<PointDto> trail, IReadOnlyList<DisplayDto> displays) =>
        dispatcher.InvokeAsync(() => Show(cursor, trail, displays)).Task;

    private void Show(PointDto cursor, IReadOnlyList<PointDto> trail, IReadOnlyList<DisplayDto> displays)
    {
        var signature = string.Join("|", displays.Select(display => $"{display.Id}:{display.Frame}"));
        if (!string.Equals(signature, displaySignature, StringComparison.Ordinal))
        {
            foreach (var window in windows.Values)
                window.Close();
            windows.Clear();
            foreach (var display in displays)
            {
                var window = new CursorOverlayWindow(display);
                windows[display.Id] = window;
                window.Show();
            }
            displaySignature = signature;
        }

        foreach (var display in displays)
            windows[display.Id].UpdateCursor(cursor, trail);
    }
}

internal sealed class CursorOverlayWindow : Window
{
    private readonly DisplayDto display;
    private readonly CursorOverlaySurface surface = new();
    private double dpiScale = 1;

    internal CursorOverlayWindow(DisplayDto display)
    {
        this.display = display;
        Content = surface;
        Background = Brushes.Transparent;
        AllowsTransparency = true;
        WindowStyle = WindowStyle.None;
        ResizeMode = ResizeMode.NoResize;
        ShowInTaskbar = false;
        ShowActivated = false;
        Focusable = false;
        Topmost = true;
        Left = display.Frame.X;
        Top = display.Frame.Y;
        Width = display.Frame.Width;
        Height = display.Frame.Height;
        SourceInitialized += OnSourceInitialized;
    }

    private void OnSourceInitialized(object? sender, EventArgs args)
    {
        var hwnd = new WindowInteropHelper(this).Handle;
        var style = NativeMethods.GetWindowLong(hwnd, NativeMethods.GwlExStyle);
        NativeMethods.SetWindowLong(
            hwnd,
            NativeMethods.GwlExStyle,
            style | NativeMethods.WsExTransparent | NativeMethods.WsExToolWindow | NativeMethods.WsExNoActivate);
        NativeMethods.SetWindowDisplayAffinity(hwnd, NativeMethods.WdaExcludeFromCapture);
        dpiScale = Math.Max(1, NativeMethods.GetDpiForWindow(hwnd)) / 96.0;
        NativeMethods.SetWindowPos(
            hwnd,
            NativeMethods.HwndTopmost,
            (int)display.Frame.X,
            (int)display.Frame.Y,
            (int)display.Frame.Width,
            (int)display.Frame.Height,
            NativeMethods.SwpNoActivate | NativeMethods.SwpShowWindow);
    }

    internal void UpdateCursor(PointDto cursor, IReadOnlyList<PointDto> trail)
    {
        surface.Update(
            new Point(
                (cursor.X - display.Frame.X) / dpiScale,
                (cursor.Y - display.Frame.Y) / dpiScale),
            trail.Select(point => new Point(
                (point.X - display.Frame.X) / dpiScale,
                (point.Y - display.Frame.Y) / dpiScale)).ToArray(),
            display.Frame.Contains(cursor));
    }
}

internal sealed class CursorOverlaySurface : FrameworkElement
{
    private Point cursor;
    private IReadOnlyList<Point> trail = [];
    private bool cursorVisible;

    internal void Update(Point newCursor, IReadOnlyList<Point> newTrail, bool visible)
    {
        cursor = newCursor;
        trail = newTrail;
        cursorVisible = visible;
        InvalidateVisual();
    }

    protected override void OnRender(DrawingContext drawingContext)
    {
        base.OnRender(drawingContext);
        var visibleTrail = trail.Where(point =>
            point.X >= -40 && point.Y >= -40 &&
            point.X <= ActualWidth + 40 && point.Y <= ActualHeight + 40).ToArray();
        CursorArtwork.DrawTrail(drawingContext, visibleTrail);
        if (cursorVisible)
            CursorArtwork.DrawPointer(drawingContext, cursor);
    }
}
