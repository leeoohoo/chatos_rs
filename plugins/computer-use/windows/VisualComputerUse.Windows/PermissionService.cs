using System.Diagnostics;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Windows.Threading;

namespace VisualComputerUse.Windows;

internal sealed class PermissionService(Dispatcher dispatcher)
{
    internal PermissionDto Diagnostics(bool onboardingPresented = false)
    {
        var interactiveDesktop = HasInputDesktop();
        var screenCapture = interactiveDesktop && CanCaptureScreen();
        var elevated = IsElevated();
        var executable = Environment.ProcessPath ?? Process.GetCurrentProcess().MainModule?.FileName ?? "unknown";
        var guidance = new List<string>
        {
            "Windows does not use the macOS Screen Recording or Accessibility permission model.",
            "Run the MCP in the same signed-in interactive desktop session as the applications it controls.",
            "To control an application running as Administrator, the MCP host must also run as Administrator because UIPI blocks lower-integrity input.",
            "Remote Desktop disconnects, the secure desktop, lock screen, UAC prompts, and some protected content cannot be captured or controlled."
        };
        return new PermissionDto(
            ScreenCaptureAvailable: screenCapture,
            InputDesktopAvailable: interactiveDesktop,
            ProcessElevated: elevated,
            AllGranted: interactiveDesktop && screenCapture,
            OnboardingPresented: onboardingPresented,
            ApplicationName: "Visual Computer Use for Windows",
            Executable: executable,
            Limitations:
            [
                "Windows requires a momentary physical cursor jump for coordinate-targeted SendInput clicks and wheel events; the original cursor position is restored immediately.",
                "SetWindowDisplayAffinity excludes the visual cursor overlay from GDI capture when supported; the marker is rendered into every returned screenshot separately."
            ],
            Permissions:
            [
                new PermissionItemDto(
                    "screen_capture",
                    "Real screen capture",
                    screenCapture,
                    "Required to observe the real desktop pixels with GDI BitBlt.",
                    screenCapture ? "Ready." : "Screen pixels are unavailable in this session. Unlock or reconnect the interactive desktop."),
                new PermissionItemDto(
                    "interactive_desktop",
                    "Interactive Windows desktop",
                    interactiveDesktop,
                    "Required for real screen pixels and SendInput events.",
                    interactiveDesktop ? "Ready." : "Sign in to an unlocked interactive desktop session and reconnect the MCP."),
                new PermissionItemDto(
                    "integrity_level",
                    "Application integrity level",
                    elevated,
                    "Administrator elevation is only required when the target application is also elevated.",
                    elevated ? "This MCP is elevated." : "For normal applications no action is needed. Relaunch the MCP host as Administrator only when controlling an elevated target.")
            ],
            Guidance: guidance);
    }

    internal async Task<PermissionDto> PresentAsync()
    {
        await dispatcher.InvokeAsync(ShowWindow);
        return Diagnostics(onboardingPresented: true);
    }

    private void ShowWindow()
    {
        var diagnostic = Diagnostics();
        var window = new Window
        {
            Title = "Visual Computer Use · Windows capability check",
            Width = 690,
            Height = 680,
            WindowStartupLocation = WindowStartupLocation.CenterScreen,
            ResizeMode = ResizeMode.NoResize,
            Topmost = true,
            Background = new SolidColorBrush(Color.FromRgb(16, 20, 31)),
            Foreground = Brushes.White,
            ShowInTaskbar = true
        };
        var panel = new StackPanel { Margin = new Thickness(34) };
        panel.Children.Add(new TextBlock
        {
            Text = "Windows capability check",
            FontSize = 26,
            FontWeight = FontWeights.SemiBold,
            Margin = new Thickness(0, 0, 0, 8)
        });
        panel.Children.Add(new TextBlock
        {
            Text = "This MCP uses real screen pixels and real SendInput events. It does not inspect DOM or application UI trees.",
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Color.FromRgb(190, 201, 221)),
            FontSize = 14,
            Margin = new Thickness(0, 0, 0, 24)
        });
        panel.Children.Add(StatusCard(
            "Real screen capture",
            diagnostic.ScreenCaptureAvailable,
            diagnostic.ScreenCaptureAvailable
                ? "GDI can read real pixels from this desktop session."
                : "Screen pixels are unavailable; unlock or reconnect the interactive desktop."));
        panel.Children.Add(StatusCard(
            "Interactive desktop",
            diagnostic.InputDesktopAvailable,
            diagnostic.InputDesktopAvailable
                ? "The signed-in desktop is available for capture and input."
                : "Unlock the Windows desktop and reconnect this MCP."));
        panel.Children.Add(StatusCard(
            "Administrator boundary",
            diagnostic.ProcessElevated,
            diagnostic.ProcessElevated
                ? "This process can control normal and elevated applications."
                : "Normal apps are supported. To control an elevated app, relaunch the MCP host as Administrator."));
        panel.Children.Add(new TextBlock
        {
            Text = $"Authorization target: {diagnostic.Executable}",
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Color.FromRgb(133, 226, 239)),
            Margin = new Thickness(0, 8, 0, 0),
            FontSize = 12
        });
        panel.Children.Add(new TextBlock
        {
            Text = "Windows has no single Screen Recording permission page. UAC prompts, the lock screen, secure desktop, DRM/protected surfaces, and disconnected RDP sessions remain outside this MCP's control.",
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Color.FromRgb(190, 201, 221)),
            Margin = new Thickness(0, 20, 0, 20),
            FontSize = 13
        });
        var close = new Button
        {
            Content = "Done",
            Width = 110,
            Height = 38,
            HorizontalAlignment = HorizontalAlignment.Right,
            Background = new SolidColorBrush(Color.FromRgb(53, 209, 232)),
            Foreground = new SolidColorBrush(Color.FromRgb(8, 19, 28)),
            FontWeight = FontWeights.SemiBold,
            BorderThickness = new Thickness(0)
        };
        close.Click += (_, _) => window.Close();
        panel.Children.Add(close);
        window.Content = panel;
        window.Show();
    }

    private static Border StatusCard(string title, bool positive, string detail)
    {
        var content = new StackPanel();
        content.Children.Add(new TextBlock
        {
            Text = $"{(positive ? "●" : "○")}  {title}",
            FontSize = 16,
            FontWeight = FontWeights.SemiBold,
            Foreground = positive
                ? new SolidColorBrush(Color.FromRgb(74, 232, 188))
                : new SolidColorBrush(Color.FromRgb(255, 193, 92))
        });
        content.Children.Add(new TextBlock
        {
            Text = detail,
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Color.FromRgb(190, 201, 221)),
            Margin = new Thickness(0, 8, 0, 0)
        });
        return new Border
        {
            Child = content,
            Background = new SolidColorBrush(Color.FromRgb(27, 33, 49)),
            CornerRadius = new CornerRadius(12),
            Padding = new Thickness(18),
            Margin = new Thickness(0, 0, 0, 12)
        };
    }

    private static bool HasInputDesktop()
    {
        const uint desktopSwitchDesktop = 0x0100;
        var desktop = NativeMethods.OpenInputDesktop(0, false, desktopSwitchDesktop);
        if (desktop == 0)
            return false;
        NativeMethods.CloseDesktop(desktop);
        return true;
    }

    private static bool CanCaptureScreen()
    {
        var hdc = NativeMethods.GetDC(0);
        if (hdc == 0)
            return false;
        try
        {
            var x = NativeMethods.GetSystemMetrics(NativeMethods.SmXVirtualScreen);
            var y = NativeMethods.GetSystemMetrics(NativeMethods.SmYVirtualScreen);
            return NativeMethods.GetPixel(hdc, x, y) != 0xFFFFFFFF;
        }
        finally
        {
            NativeMethods.ReleaseDC(0, hdc);
        }
    }

    private static bool IsElevated()
    {
        if (!NativeMethods.OpenProcessToken(Process.GetCurrentProcess().Handle, NativeMethods.TokenQuery, out var token))
            return false;
        try
        {
            return NativeMethods.GetTokenInformation(
                token,
                NativeMethods.TokenElevationClass,
                out var elevation,
                System.Runtime.InteropServices.Marshal.SizeOf<NativeMethods.TokenElevation>(),
                out _) && elevation.TokenIsElevated != 0;
        }
        finally
        {
            NativeMethods.CloseHandle(token);
        }
    }
}
