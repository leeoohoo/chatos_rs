using System.Runtime.InteropServices;
using System.Text;

namespace ChatOS.Desktop;

internal static class StartupDiagnostics
{
    private const uint ErrorIcon = 0x00000010;
    private static readonly object Sync = new();
    private static int _fatalDialogShown;

    public static string LogPath { get; } = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "ChatOS",
        "logs",
        "startup.log");

    public static void Initialize()
    {
        WriteLine($"ChatOS startup initiated. Version={typeof(App).Assembly.GetName().Version}; " +
            $"OS={Environment.OSVersion}; ProcessArchitecture={RuntimeInformation.ProcessArchitecture}");
    }

    public static void RecordUnhandled(string phase, Exception exception) =>
        WriteLine($"Unhandled exception during {phase}:{Environment.NewLine}{exception}");

    public static void ReportFatal(string phase, Exception exception)
    {
        RecordUnhandled(phase, exception);
        if (Interlocked.Exchange(ref _fatalDialogShown, 1) != 0) return;

        var message = $"ChatOS could not start.\n\nA diagnostic log was written to:\n{LogPath}";
        try
        {
            _ = MessageBoxW(IntPtr.Zero, message, "ChatOS startup failed", ErrorIcon);
        }
        catch
        {
            // The file log remains available even if Windows cannot show the dialog.
        }
    }

    private static void WriteLine(string message)
    {
        try
        {
            lock (Sync)
            {
                var directory = Path.GetDirectoryName(LogPath);
                if (!string.IsNullOrWhiteSpace(directory)) Directory.CreateDirectory(directory);
                File.AppendAllText(
                    LogPath,
                    $"[{DateTimeOffset.Now:O}] {message}{Environment.NewLine}",
                    new UTF8Encoding(false));
            }
        }
        catch
        {
            // Diagnostics must never replace the original startup failure.
        }
    }

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern int MessageBoxW(IntPtr window, string text, string caption, uint type);
}
