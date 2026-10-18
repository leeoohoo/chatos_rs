namespace ChatOS.Connector.Tests;

public sealed class DesktopNativeCompatibilityContractTests
{
    [Fact]
    public void ChatLongTextPasteIsHandledBeforeClipboardAwait()
    {
        var source = File.ReadAllText(Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Desktop",
            "Features",
            "Chat",
            "ConversationPage.xaml.cs"));
        var textBranch = source.IndexOf(
            "if (!content.Contains(StandardDataFormats.Text))",
            StringComparison.Ordinal);
        var handled = source.IndexOf("e.Handled = true;", textBranch, StringComparison.Ordinal);
        var clipboardAwait = source.IndexOf(
            "await content.GetTextAsync()",
            textBranch,
            StringComparison.Ordinal);

        Assert.True(textBranch >= 0);
        Assert.True(handled > textBranch && handled < clipboardAwait,
            "The TextBox must own the paste before asynchronously reading clipboard text.");
        Assert.Contains("ConversationComposerTextPaste.Insert(", source, StringComparison.Ordinal);
    }

    [Fact]
    public void ChatClipboardBitmapIsEncodedAsPngBeforeAttachmentCreation()
    {
        var source = File.ReadAllText(Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Desktop",
            "Features",
            "Chat",
            "ConversationPage.xaml.cs"));

        Assert.Contains("NormalizeClipboardImageToPngAsync(bitmap)", source, StringComparison.Ordinal);
        Assert.Contains("BitmapEncoder.PngEncoderId", source, StringComparison.Ordinal);
        Assert.Contains("\"image/png\"", source, StringComparison.Ordinal);
        Assert.DoesNotContain("var mimeType = string.IsNullOrWhiteSpace(stream.ContentType)", source, StringComparison.Ordinal);
    }

    [Fact]
    public void GlobalUtilitiesRegisterFourIndependentShortcutsAndRouteTheirActions()
    {
        var windowsRoot = FindWindowsRoot();
        var service = File.ReadAllText(Path.Combine(
            windowsRoot,
            "src",
            "ChatOS.Desktop",
            "Features",
            "QuickSearch",
            "WindowsGlobalHotKeyService.cs"));
        var coordinator = File.ReadAllText(Path.Combine(
            windowsRoot,
            "src",
            "ChatOS.Desktop",
            "Features",
            "QuickSearch",
            "QuickSearchCoordinator.cs"));

        Assert.Contains("WindowsGlobalUtilityAction.Screenshot", service, StringComparison.Ordinal);
        Assert.Contains("WindowsGlobalUtilityAction.ScreenRecording", service, StringComparison.Ordinal);
        Assert.Contains("WindowsGlobalUtilityAction.ClipboardHistory", service, StringComparison.Ordinal);
        Assert.Contains("WindowsGlobalUtilityAction.QuickSearch", service, StringComparison.Ordinal);
        Assert.Contains("Ctrl+Shift+A", service, StringComparison.Ordinal);
        Assert.Contains("Ctrl+Shift+Q", service, StringComparison.Ordinal);
        Assert.Contains("Ctrl+Shift+E", service, StringComparison.Ordinal);
        Assert.Contains("Ctrl+Alt+Space", service, StringComparison.Ordinal);
        Assert.Contains("preferences.Changed += OnPreferencesChanged;", service, StringComparison.Ordinal);
        Assert.Contains("QuickSearchBuiltInAction.Screenshot", coordinator, StringComparison.Ordinal);
        Assert.Contains("QuickSearchBuiltInAction.ScreenRecording", coordinator, StringComparison.Ordinal);
        Assert.Contains("QuickSearchBuiltInAction.ClipboardHistory", coordinator, StringComparison.Ordinal);
    }

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
    public void MediaStudioSafelyWritesBackDynamicVideoDurationSelection()
    {
        var windowsRoot = FindWindowsRoot();
        var xaml = File.ReadAllText(Path.Combine(
            windowsRoot,
            "src",
            "ChatOS.Desktop",
            "Features",
            "MediaStudio",
            "MediaStudioPage.xaml"));
        var codeBehind = File.ReadAllText(Path.Combine(
            windowsRoot,
            "src",
            "ChatOS.Desktop",
            "Features",
            "MediaStudio",
            "MediaStudioPage.xaml.cs"));

        Assert.DoesNotContain(
            "SelectedItem=\"{x:Bind ViewModel.VideoSeconds, Mode=TwoWay}\"",
            xaml,
            StringComparison.Ordinal);
        Assert.Contains(
            "SelectionChanged=\"OnVideoDurationSelectionChanged\"",
            xaml,
            StringComparison.Ordinal);
        Assert.Contains(
            "sender is ComboBox { SelectedItem: int seconds }",
            codeBehind,
            StringComparison.Ordinal);
        Assert.DoesNotContain(
            "SelectedItem=\"{x:Bind ViewModel.VideoSize, Mode=TwoWay}\"",
            xaml,
            StringComparison.Ordinal);
        Assert.Contains(
            "SelectionChanged=\"OnVideoSizeSelectionChanged\"",
            xaml,
            StringComparison.Ordinal);
        Assert.Contains(
            "sender is ComboBox { SelectedItem: string { Length: > 0 } size }",
            codeBehind,
            StringComparison.Ordinal);
    }

    [Fact]
    public void LocalNotepadIsPreparedBeforeConnectorDependentAgentBootstrap()
    {
        var source = File.ReadAllText(Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Connector",
            "LocalAgent",
            "WindowsLocalAgentBootstrapService.cs"));

        var hostStart = source.IndexOf(
            "await _host.StartForBootstrapAsync(ownerUserId, cancellationToken)",
            StringComparison.Ordinal);
        var notepadConfiguration = source.IndexOf(
            "ConfigureOwnerServices(ownerUserId);",
            StringComparison.Ordinal);
        var connectorSession = source.IndexOf(
            ".SessionConfigurationAsync(cancellationToken)",
            StringComparison.Ordinal);

        Assert.True(hostStart >= 0, "The Local Agent Host must be started for local storage.");
        Assert.True(notepadConfiguration > hostStart,
            "The notepad must be bound after the native host is ready.");
        Assert.True(connectorSession > notepadConfiguration,
            "Local notepad setup must not depend on Connector session bootstrap.");

        var ownerConfiguration = source.IndexOf(
            "private void ConfigureOwnerServices(string ownerUserId)",
            StringComparison.Ordinal);
        Assert.True(ownerConfiguration >= 0);
        Assert.Contains("_workspace.Configure(ownerUserId);", source, StringComparison.Ordinal);
        Assert.Contains("_projectConversations.Configure(ownerUserId);", source, StringComparison.Ordinal);
        Assert.Contains("_notepad.Configure(ownerUserId);", source, StringComparison.Ordinal);
    }

    [Fact]
    public void UnexpectedHostRecoveryDoesNotExposeAnUncredentialedHostToConsumers()
    {
        var source = File.ReadAllText(Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Connector",
            "LocalAgent",
            "WindowsLocalAgentBootstrapService.cs"));

        Assert.Contains(
            "configureLocalServicesBeforeModelBootstrap: false",
            source,
            StringComparison.Ordinal);
        var credentialRestart = source.IndexOf(
            "await _host.RefreshCredentialsForOwnerAsync(",
            StringComparison.Ordinal);
        var deferredConfiguration = source.IndexOf(
            "if (!configureLocalServicesBeforeModelBootstrap)",
            credentialRestart,
            StringComparison.Ordinal);

        Assert.True(credentialRestart >= 0);
        Assert.True(deferredConfiguration > credentialRestart,
            "Recovered consumers must only be rebound after the credentialed Host restart.");
    }

    [Fact]
    public void LocalAgentStorageIsPreparedBeforeWorkspaceReload()
    {
        var source = File.ReadAllText(Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Desktop",
            "AppShell",
            "MainWindowViewModel.cs"));

        var restorePreparation = source.IndexOf(
            "await PrepareLocalAgentServicesAsync(session.User.Id, cancellationToken);",
            StringComparison.Ordinal);
        var restoreReload = source.IndexOf(
            "await ReloadWorkspaceCoreAsync(cancellationToken);",
            StringComparison.Ordinal);
        var loginPreparation = source.IndexOf(
            "await PrepareLocalAgentServicesAsync(session.User.Id, CancellationToken.None);",
            StringComparison.Ordinal);
        var loginReload = source.IndexOf(
            "await ReloadWorkspaceCoreAsync();",
            StringComparison.Ordinal);

        Assert.True(restorePreparation >= 0 && restorePreparation < restoreReload);
        Assert.True(loginPreparation >= 0 && loginPreparation < loginReload);
    }

    [Fact]
    public void RequiredExternalMcpSelectionMatchesTheMacHttpRuntimePolicy()
    {
        var source = File.ReadAllText(Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Connector",
            "LocalAgent",
            "WindowsLocalAgentBootstrapService.cs"));

        Assert.Contains(
            "value.Resource.Runtime.Kind.Equals(\"http\", StringComparison.OrdinalIgnoreCase)",
            source,
            StringComparison.Ordinal);
        Assert.Contains(
            "NonEmpty(value.Resource.Runtime.Url) is not null",
            source,
            StringComparison.Ordinal);
        Assert.DoesNotContain(
            "A required external MCP cannot execute on this client.",
            source,
            StringComparison.Ordinal);
    }

    [Fact]
    public void PluginTaskPolicySerializesFrozenComponentsAndReleaseSnapshots()
    {
        var source = File.ReadAllText(Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Connector",
            "LocalAgent",
            "WindowsLocalAgentBootstrapService.cs"));

        Assert.Contains("plugin_components = requiredPluginComponents", source,
            StringComparison.Ordinal);
        Assert.Contains("plugin_release_snapshots = pluginReleaseSnapshots", source,
            StringComparison.Ordinal);
        Assert.DoesNotContain("plugin_keys = requiredPluginKeys", source,
            StringComparison.Ordinal);
        Assert.Contains("component_key = value.ComponentKey", source,
            StringComparison.Ordinal);
        Assert.Contains("artifact_sha256 = value.ArtifactSha256", source,
            StringComparison.Ordinal);
    }

    [Fact]
    public void PluginExecutorLaunchesOnlyTheFrozenComponent()
    {
        var source = File.ReadAllText(Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Connector",
            "LocalAgent",
            "WindowsLocalAgentPluginToolExecutor.cs"));
        var loadStart = source.IndexOf("private async Task<LoadedPlugin> LoadAsync(",
            StringComparison.Ordinal);
        var validationStart = source.IndexOf(
            "internal static WindowsLocalAgentPluginChoice ValidateFrozenChoice(",
            loadStart,
            StringComparison.Ordinal);

        Assert.True(loadStart >= 0 && validationStart > loadStart);
        var loadSource = source[loadStart..validationStart].Replace("\r\n", "\n");
        Assert.Contains("var componentKey = option.Choice.ComponentKey;", loadSource,
            StringComparison.Ordinal);
        Assert.Contains("option.Record,\n                componentKey,", loadSource,
            StringComparison.Ordinal);
        Assert.DoesNotContain("ListMcpComponentChoicesAsync", loadSource,
            StringComparison.Ordinal);
        Assert.DoesNotContain("manifest.McpServers", loadSource,
            StringComparison.Ordinal);
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
        Assert.Contains("x:Name=\"StoryCreateOverlay\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnOpenStoryCreateClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnCancelStoryCreateClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("AutomationProperties.Name=\"提示词中心\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"个剧情项目\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"创建剧情，选择三类模型，再把完整故事拆成 2–15 秒剧情段与必要的独立转场。\"", xaml, StringComparison.Ordinal);
        Assert.Contains("StoryProjectStatusBrushConverter", xaml, StringComparison.Ordinal);
        Assert.DoesNotContain("x:Name=\"StoryCreateForm\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnAddGeneratedReferenceClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"StoryPromptAuditExpander\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnStoryHistoryPlayClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"已自动保存到本机，重启后仍可查看\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"图片和视频按所属剧情集中展示，分段视频可按顺序连续播放。\"", xaml, StringComparison.Ordinal);
        Assert.Contains("ItemsSource=\"{x:Bind HistoryImages}\"", xaml, StringComparison.Ordinal);
        Assert.Contains("ItemsSource=\"{x:Bind HistoryVideos}\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnStoryHistoryImageClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnStoryHistoryVideoClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnRevealStoryHistoryVideoClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"单独生成的视频\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"单独生成的图片\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnRevealVideoHistoryClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnRevealImageHistoryClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"HistoryLoadingState\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:Name=\"StoryLandingLoadingState\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Value=\"{x:Bind CompletionRatio}\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnPickGeneratedVideoFrameClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnClearReferencesClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"{x:Bind ViewModel.GenerateVideoActionLabel, Mode=OneWay}\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"{x:Bind ViewModel.ImageCanvasSubtitle, Mode=OneWay}\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"{x:Bind ViewModel.VideoCanvasSubtitle, Mode=OneWay}\"", xaml, StringComparison.Ordinal);
        Assert.Contains("没有可用的图片模型", xaml, StringComparison.Ordinal);
        Assert.Contains("当前没有兼容的视频模型", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"首帧参考图（可选）\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"从创作记录选择\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"添加首帧参考图\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"添加参考音频\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"正在读取参考图…\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"正在读取参考音频…\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"描述一段有运动和镜头感的画面\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"视频任务会在模型提供方异步生成，完成后自动下载到本机。\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"{x:Bind FileName}\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnVideoHistoryCardClick\" ToolTipService.ToolTip=\"播放视频\"", xaml, StringComparison.Ordinal);
        Assert.Contains("x:DataType=\"media:MediaStudioReferenceImage\"", xaml, StringComparison.Ordinal);
        Assert.Contains("ViewModel.VideoFirstFramePreviewPath", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"{x:Bind ViewModel.GenerateImageActionLabel, Mode=OneWay}\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"{x:Bind ViewModel.ImageEmptyDescription, Mode=OneWay}\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"创作画布\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"描述你想看到的画面\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"从生成记录选择多张\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"最多 8 张；每张最大 20 MB\"", xaml, StringComparison.Ordinal);
        Assert.Contains("IsEnabled=\"{x:Bind ViewModel.CanReloadModels, Mode=OneWay}\"", xaml,
            StringComparison.Ordinal);
        Assert.Contains("x:Key=\"GeneratedImagePickerItemTemplate\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Source=\"{Binding FilePath, Converter={StaticResource FilePathToImageSourceConverter}}\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Click=\"OnPreviewImageClick\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Source=\"{Binding CoverImage.FilePath, Converter={StaticResource FilePathToImageSourceConverter}}\"", xaml, StringComparison.Ordinal);
        Assert.Contains("Text=\"{x:Bind ImageCountLabel}\"", xaml, StringComparison.Ordinal);

        var document = System.Xml.Linq.XDocument.Parse(xaml);
        var storySidebar = document.Descendants().Single(element => element.Attributes().Any(attribute =>
            attribute.Name.LocalName == "Name" && attribute.Value == "StoryProjectSidebar"));
        var storyPivot = storySidebar.Ancestors().First(element => element.Name.LocalName == "PivotItem");
        Assert.Equal("剧情模式", storyPivot.Attribute("Header")?.Value);
    }

    [Fact]
    public void ReleaseBuildsUseTheSupportedAwsLcNasmFallback()
    {
        var windowsRoot = FindWindowsRoot();
        var build = File.ReadAllText(Path.Combine(windowsRoot, "build", "build.ps1"));
        var package = File.ReadAllText(Path.Combine(windowsRoot, "build", "package.ps1"));

        Assert.Contains("if ($Configuration -eq \"Release\")", build, StringComparison.Ordinal);
        Assert.Contains("$env:AWS_LC_SYS_NO_ASM = $null", build, StringComparison.Ordinal);
        Assert.Contains("$env:AWS_LC_SYS_PREBUILT_NASM = \"1\"", build, StringComparison.Ordinal);
        Assert.Contains("$env:AWS_LC_SYS_PREBUILT_NASM = $previousAwsLcPrebuiltNasm", build,
            StringComparison.Ordinal);
        Assert.Contains("$env:AWS_LC_SYS_NO_ASM = $null", package, StringComparison.Ordinal);
        Assert.Contains("$env:AWS_LC_SYS_PREBUILT_NASM = \"1\"", package,
            StringComparison.Ordinal);
        Assert.Contains("Push-Location $monorepoRoot", package, StringComparison.Ordinal);
        Assert.Contains("$desktopProject,", package, StringComparison.Ordinal);
        Assert.Contains("$env:AWS_LC_SYS_PREBUILT_NASM = $previousAwsLcPrebuiltNasm", package,
            StringComparison.Ordinal);
    }

    [Fact]
    public void PackagedDesktopDeclaresRuntimeHostPetAndBilingualResources()
    {
        var windowsRoot = FindWindowsRoot();
        var project = File.ReadAllText(Path.Combine(
            windowsRoot,
            "src",
            "ChatOS.Desktop",
            "ChatOS.Desktop.csproj"));
        var manifest = System.Xml.Linq.XDocument.Load(Path.Combine(
            windowsRoot,
            "src",
            "ChatOS.Desktop",
            "Package.appxmanifest"));

        Assert.Contains("Include=\"chatos.runtime.json\"", project, StringComparison.Ordinal);
        Assert.Contains("Link=\"chatos_local_agent_host.exe\"", project, StringComparison.Ordinal);
        Assert.Contains("Assets\\Pets\\fengtuan\\frames\\*.png", project, StringComparison.Ordinal);
        Assert.Contains("CopyToPublishDirectory=\"PreserveNewest\"", project,
            StringComparison.Ordinal);

        var languages = manifest.Descendants()
            .Where(element => element.Name.LocalName == "Resource")
            .Select(element => (string?)element.Attribute("Language"))
            .ToHashSet(StringComparer.OrdinalIgnoreCase);
        Assert.Contains("zh-CN", languages);
        Assert.Contains("en-US", languages);
    }

    [Fact]
    public void DesktopHostConfigurationIsAnchoredToThePackagedApplicationDirectory()
    {
        var source = File.ReadAllText(Path.Combine(
            FindWindowsRoot(),
            "src",
            "ChatOS.Desktop",
            "App.xaml.cs"));

        Assert.Contains("Host.CreateApplicationBuilder(new HostApplicationBuilderSettings", source,
            StringComparison.Ordinal);
        Assert.Contains("ContentRootPath = AppContext.BaseDirectory", source,
            StringComparison.Ordinal);
        Assert.DoesNotContain("Host.CreateApplicationBuilder();", source, StringComparison.Ordinal);
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
