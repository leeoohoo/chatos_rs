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

    [Fact]
    public void DesktopUsesWindowsAppSdk18OrNewerRuntimeLine()
    {
        var packages = System.Xml.Linq.XDocument.Load(Path.Combine(
            FindWindowsRoot(),
            "Directory.Packages.props"));
        var package = packages.Descendants("PackageVersion").Single(element =>
            string.Equals(
                (string?)element.Attribute("Include"),
                "Microsoft.WindowsAppSDK",
                StringComparison.Ordinal));
        var rawVersion = (string?)package.Attribute("Version");
        var version = Version.Parse(rawVersion ?? throw new InvalidDataException(
            "Microsoft.WindowsAppSDK package version is missing."));

        Assert.True(version >= new Version(1, 8), $"Unsupported Windows App SDK: {rawVersion}");
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
