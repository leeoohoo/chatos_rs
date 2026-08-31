using System.Diagnostics;
using System.IO;
using System.Text;
using System.Windows.Threading;

namespace VisualComputerUse.Windows;

internal sealed class ComputerController
{
    private readonly CursorOverlayManager overlay;
    private readonly ScreenCaptureService capture;
    private readonly PermissionService permissions;
    private PointDto? virtualCursor;
    private IReadOnlyList<PointDto> trail = [];

    internal ComputerController(Dispatcher dispatcher)
    {
        overlay = new CursorOverlayManager(dispatcher);
        capture = new ScreenCaptureService(dispatcher);
        permissions = new PermissionService(dispatcher);
    }

    internal PermissionDto CheckPermissions() => permissions.Diagnostics();

    internal Task<PermissionDto> RequestPermissionsAsync() => permissions.PresentAsync();

    internal async Task<CapturedObservation> ObserveAsync(ObservationOptions options)
    {
        var displays = DisplayService.GetDisplays();
        var cursor = GetVirtualCursor(displays);
        await overlay.ShowAsync(cursor, trail, displays).ConfigureAwait(false);
        return await capture.CaptureAsync(
            options,
            cursor,
            trail,
            displays,
            ActiveApplication()).ConfigureAwait(false);
    }

    internal async Task MoveVirtualCursorAsync(PointDto target, double duration, int steps)
    {
        var displays = DisplayService.GetDisplays();
        ValidatePoint(target, displays);
        var start = GetVirtualCursor(displays);
        var trajectory = VirtualTrajectory(start, target, steps);
        duration = Math.Clamp(duration, 0, 3);
        for (var index = 0; index < trajectory.Count; index++)
        {
            virtualCursor = trajectory[index];
            trail = trajectory.Take(index + 1).ToArray();
            await overlay.ShowAsync(virtualCursor, trail, displays).ConfigureAwait(false);
            if (duration > 0 && index + 1 < trajectory.Count)
                await Task.Delay(TimeSpan.FromSeconds(duration / (trajectory.Count - 1))).ConfigureAwait(false);
        }
    }

    internal PointDto CurrentVirtualCursor()
    {
        var displays = DisplayService.GetDisplays();
        return GetVirtualCursor(displays);
    }

    internal async Task ClickAsync(string button, int count, double interval)
    {
        var point = CurrentVirtualCursor();
        InputService.Click(point, button, count, interval);
        trail = [];
        await overlay.ShowAsync(point, trail, DisplayService.GetDisplays()).ConfigureAwait(false);
    }

    internal async Task ScrollAsync(int deltaX, int deltaY, double duration, int steps)
    {
        var point = CurrentVirtualCursor();
        trail = [];
        await overlay.ShowAsync(point, trail, DisplayService.GetDisplays()).ConfigureAwait(false);
        await InputService.ScrollAsync(point, deltaX, deltaY, duration, steps).ConfigureAwait(false);
    }

    internal void TypeText(string text)
    {
        trail = [];
        InputService.TypeText(text);
    }

    internal void PressKeys(IReadOnlyList<string> keys)
    {
        trail = [];
        InputService.PressKeys(keys);
    }

    internal ActiveApplicationDto ActiveApplication()
    {
        var hwnd = NativeMethods.GetForegroundWindow();
        if (hwnd == 0)
            return new ActiveApplicationDto(null, null, null, null);
        NativeMethods.GetWindowThreadProcessId(hwnd, out var processId);
        var title = new StringBuilder(1024);
        NativeMethods.GetWindowText(hwnd, title, title.Capacity);
        try
        {
            using var process = Process.GetProcessById((int)processId);
            return new ActiveApplicationDto(
                process.ProcessName,
                TryGetExecutable(process),
                process.Id,
                title.ToString());
        }
        catch
        {
            return new ActiveApplicationDto(null, null, (int)processId, title.ToString());
        }
    }

    internal async Task ActivateApplicationAsync(string application)
    {
        var normalized = Path.GetFileNameWithoutExtension(application);
        var existing = Process.GetProcessesByName(normalized)
            .FirstOrDefault(process => process.MainWindowHandle != 0);
        if (existing is not null)
        {
            using (existing)
            {
                NativeMethods.ShowWindow(existing.MainWindowHandle, 9);
                if (!NativeMethods.SetForegroundWindow(existing.MainWindowHandle))
                    throw new VisualComputerUseException($"Windows refused to foreground '{application}'. Try using its visible taskbar window.");
            }
            return;
        }

        Process? launched;
        try
        {
            launched = Process.Start(new ProcessStartInfo(application) { UseShellExecute = true });
        }
        catch (Exception error)
        {
            throw new VisualComputerUseException($"Could not launch '{application}': {error.Message}");
        }
        if (launched is null)
            throw new VisualComputerUseException($"Could not launch '{application}'.");

        using (launched)
        {
            for (var attempt = 0; attempt < 30; attempt++)
            {
                await Task.Delay(100).ConfigureAwait(false);
                launched.Refresh();
                if (launched.MainWindowHandle == 0)
                    continue;
                NativeMethods.ShowWindow(launched.MainWindowHandle, 9);
                NativeMethods.SetForegroundWindow(launched.MainWindowHandle);
                return;
            }
        }
    }

    internal static IReadOnlyList<PointDto> VirtualTrajectory(PointDto start, PointDto target, int steps)
    {
        var count = Math.Clamp(steps, 2, 80);
        var dx = target.X - start.X;
        var dy = target.Y - start.Y;
        var distance = Math.Sqrt(dx * dx + dy * dy);
        if (distance <= 0.5)
            return [start, target];
        var normalX = -dy / distance;
        var normalY = dx / distance;
        var arc = Math.Min(90, distance * 0.13);
        var points = new PointDto[count];
        for (var index = 0; index < count; index++)
        {
            var t = (double)index / (count - 1);
            var eased = t * t * (3 - 2 * t);
            var curve = Math.Sin(Math.PI * t) * arc;
            points[index] = new PointDto(
                start.X + dx * eased + normalX * curve,
                start.Y + dy * eased + normalY * curve);
        }
        return points;
    }

    private PointDto GetVirtualCursor(IReadOnlyList<DisplayDto> displays)
    {
        if (virtualCursor is not null && displays.Any(display => display.Frame.Contains(virtualCursor)))
            return virtualCursor;
        if (NativeMethods.GetCursorPos(out var physical))
        {
            var point = new PointDto(physical.X, physical.Y);
            if (displays.Any(display => display.Frame.Contains(point)))
            {
                virtualCursor = point;
                return point;
            }
        }
        var primary = displays.FirstOrDefault(display => display.IsPrimary) ?? displays.First();
        virtualCursor = new PointDto(
            primary.Frame.X + primary.Frame.Width / 2,
            primary.Frame.Y + primary.Frame.Height / 2);
        return virtualCursor;
    }

    private static void ValidatePoint(PointDto point, IReadOnlyList<DisplayDto> displays)
    {
        if (!displays.Any(display => display.Frame.Contains(point)))
            throw new VisualComputerUseException($"Point ({point.X}, {point.Y}) is outside every active display.");
    }

    private static string? TryGetExecutable(Process process)
    {
        try { return process.MainModule?.FileName; }
        catch { return null; }
    }
}
