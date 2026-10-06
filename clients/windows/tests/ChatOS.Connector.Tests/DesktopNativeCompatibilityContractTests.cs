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

    [Fact]
    public void MediaStudioDoesNotForceBindingsForUnrealizedPivotContent()
    {
        var source = File.ReadAllText(Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Desktop",
            "Features",
            "MediaStudio",
            "MediaStudioPage.xaml.cs"));

        Assert.DoesNotContain("Bindings.Update();", source, StringComparison.Ordinal);
    }

    [Fact]
    public void MediaStudioKeepsHistoryUnifiedAndStoryWorkspaceSectioned()
    {
        var xaml = File.ReadAllText(Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Desktop",
            "Features",
            "MediaStudio",
            "MediaStudioPage.xaml"));

        Assert.DoesNotContain("x:Name=\"HistoryPivot\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"StoryHistorySection\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"VideoHistorySection\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"ImageHistorySection\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"StorySourceSectionItem\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"StoryStyleSectionItem\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"StoryPortraitsSectionItem\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"StorySegmentsSectionItem\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"StoryGraphSectionItem\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"StoryLandingProjects\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnAddGeneratedReferenceClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"StoryPromptAuditExpander\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnStoryHistoryPlayClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("ItemsSource=\"{x:Bind PreviewImages}\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"HistoryLoadingState\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"StoryLandingLoadingState\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Value=\"{x:Bind CompletionRatio}\"", xaml, StringComparison.Ordinal);

        var document = System.Xml.Linq.XDocument.Parse(xaml);
        var storySidebar = document.Descendants().Single(element => element.Attributes().Any(attribute =>
            attribute.Name.LocalName == "Name" && attribute.Value == "StoryProjectSidebar"));
        var storyPivot = storySidebar.Ancestors().First(element => element.Name.LocalName == "PivotItem");
        Assert.Equal("剧情模式", storyPivot.Attribute("Header")?.Value);
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
