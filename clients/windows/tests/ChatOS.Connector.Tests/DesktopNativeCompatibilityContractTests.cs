namespace ChatOS.Connector.Tests;

public sealed class DesktopNativeCompatibilityContractTests
{
    [Fact]
    public void DesktopStartupDoesNotSubscribeToWindowsAppSdkPowerManager()
    {
        var source = File.ReadAllText(Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Desktop",
            "App.xaml.cs"));

        Assert.DoesNotContain("Microsoft.Windows.System.Power", source, StringComparison.Ordinal);
        Assert.DoesNotContain("PowerManager.", source, StringComparison.Ordinal);
    }

    private static string FindWindowsRoot()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null)
        {
            if (File.Exists(Path.Combine(directory.FullName, "ChatOS.Win.sln")))
            {
                return directory.FullName;
            }

            directory = directory.Parent;
        }

        throw new DirectoryNotFoundException("Could not locate the ChatOS Windows repository root.");
    }
}
