using System.IO;
using System.Text.Json;
using System.Windows;

namespace VisualComputerUse.Windows;

internal static class Program
{
    [STAThread]
    private static int Main(string[] args)
    {
        NativeMethods.SetProcessDpiAwarenessContext(NativeMethods.DpiAwarenessContextPerMonitorAwareV2);
        var application = new Application { ShutdownMode = ShutdownMode.OnExplicitShutdown };
        var controller = new ComputerController(application.Dispatcher);

        if (args.Contains("--doctor", StringComparer.OrdinalIgnoreCase))
        {
            var json = JsonSerializer.Serialize(controller.CheckPermissions(), McpService.JsonOptions);
            using var writer = new StreamWriter(Console.OpenStandardOutput()) { AutoFlush = true };
            writer.WriteLine(json);
            return 0;
        }

        if (args.Contains("--onboarding", StringComparer.OrdinalIgnoreCase))
        {
            application.ShutdownMode = ShutdownMode.OnLastWindowClose;
            _ = application.Dispatcher.InvokeAsync(async () => await controller.RequestPermissionsAsync());
            return application.Run();
        }

        var server = new StdioMcpServer(new McpService(controller));
        _ = Task.Run(async () =>
        {
            try
            {
                await server.RunAsync().ConfigureAwait(false);
            }
            finally
            {
                _ = application.Dispatcher.BeginInvoke(new Action(application.Shutdown));
            }
        });
        return application.Run();
    }
}
