using ChatOS.Connector.Plugins;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Desktop.Features.Settings;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed class CustomPluginInstallerViewModelTests
{
    [Fact]
    public async Task Preflight_exposes_exact_tools_and_source_change_invalidates_fingerprint()
    {
        var installer = new FakeInstaller();
        var viewModel = CreateViewModel(installer);

        Assert.False(viewModel.CanInspect);
        Assert.False(viewModel.CanInstall);

        viewModel.SourcePath = @"C:\fixtures\plugin-one";
        await viewModel.InspectAsync();

        Assert.Equal(@"C:\fixtures\plugin-one", Assert.Single(installer.Inspected).Location);
        Assert.True(viewModel.HasInspection);
        Assert.True(viewModel.CanInstall);
        Assert.Contains("ping", viewModel.InspectionComponents, StringComparison.Ordinal);
        Assert.Contains(installer.Inspection.Fingerprint, viewModel.InspectionFingerprint, StringComparison.Ordinal);
        Assert.Null(viewModel.ErrorMessage);

        viewModel.SourcePath = @"C:\fixtures\plugin-two";

        Assert.False(viewModel.HasInspection);
        Assert.False(viewModel.CanInstall);
        Assert.Null(viewModel.ActionMessage);
    }

    [Fact]
    public async Task Confirmed_install_uses_preflight_fingerprint_and_clears_stale_install_action()
    {
        var installer = new FakeInstaller();
        var viewModel = CreateViewModel(installer);
        viewModel.SourcePath = @"C:\fixtures\plugin-one";
        await viewModel.InspectAsync();

        var installed = await viewModel.InstallAsync();

        Assert.True(installed);
        var request = Assert.Single(installer.Installed);
        Assert.Equal(installer.Inspection.Fingerprint, request.Fingerprint);
        Assert.Equal(@"C:\fixtures\plugin-one", request.Source.Location);
        Assert.False(viewModel.HasInspection);
        Assert.False(viewModel.CanInstall);
        Assert.Contains("来源指纹", viewModel.ActionMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Preflight_failure_is_visible_and_does_not_enable_install()
    {
        var installer = new FakeInstaller
        {
            InspectError = new PluginPackageException("source rejected"),
        };
        var viewModel = CreateViewModel(installer);
        viewModel.SourcePath = @"C:\fixtures\invalid";

        await viewModel.InspectAsync();

        Assert.Equal("source rejected", viewModel.ErrorMessage);
        Assert.False(viewModel.IsBusy);
        Assert.False(viewModel.HasInspection);
        Assert.False(viewModel.CanInstall);
    }

    [Fact]
    public async Task Leaving_settings_cancels_preflight_and_clears_the_previous_fingerprint()
    {
        var installer = new FakeInstaller();
        var viewModel = CreateViewModel(installer);
        viewModel.SourcePath = @"C:\fixtures\plugin-one";
        await viewModel.InspectAsync();
        Assert.True(viewModel.HasInspection);

        installer.WaitForCancellation = true;
        var inspection = viewModel.InspectAsync();
        Assert.True(viewModel.IsBusy);
        Assert.False(viewModel.HasInspection);

        viewModel.CancelPending();
        await inspection;

        Assert.False(viewModel.IsBusy);
        Assert.False(viewModel.HasInspection);
        Assert.False(viewModel.CanInstall);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task Pinned_npm_preflight_and_install_use_the_dedicated_safe_pipeline()
    {
        var installer = new FakeInstaller();
        var viewModel = CreateViewModel(installer);
        viewModel.UseNpmPackage = true;
        viewModel.NpmPackageName = "rea-agents";
        viewModel.NpmPackageVersion = "6.5.0";
        viewModel.NpmBinaryName = "rea";
        viewModel.NpmComponentKey = "reverse-engineer-anything";
        viewModel.NpmDisplayName = "REA";
        viewModel.NpmDescription = "Pinned REA MCP";
        viewModel.NpmArgumentsJson = "[\"mcp\",\"--stdio\"]";

        Assert.True(viewModel.CanInspect);
        await viewModel.InspectAsync();

        var inspected = Assert.Single(installer.NpmInspected);
        Assert.Equal("rea-agents", inspected.PackageName);
        Assert.Equal("6.5.0", inspected.PackageVersion);
        Assert.Equal(["mcp", "--stdio"], inspected.Arguments);
        Assert.True(viewModel.CanInstall);

        Assert.True(await viewModel.InstallAsync());
        var installed = Assert.Single(installer.NpmInstalled);
        Assert.Equal(inspected.PackageName, installed.Draft.PackageName);
        Assert.Equal(inspected.PackageVersion, installed.Draft.PackageVersion);
        Assert.Equal(inspected.BinaryName, installed.Draft.BinaryName);
        Assert.Equal(inspected.ComponentKey, installed.Draft.ComponentKey);
        Assert.Equal(inspected.Arguments, installed.Draft.Arguments);
        Assert.Equal(installer.Inspection.Fingerprint, installed.Fingerprint);
        Assert.Empty(installer.Inspected);
        Assert.Empty(installer.Installed);
    }

    [Fact]
    public async Task Invalid_npm_argument_json_fails_before_package_acquisition()
    {
        var installer = new FakeInstaller();
        var viewModel = CreateViewModel(installer);
        viewModel.UseNpmPackage = true;
        viewModel.NpmPackageName = "rea-agents";
        viewModel.NpmPackageVersion = "6.5.0";
        viewModel.NpmBinaryName = "rea";
        viewModel.NpmComponentKey = "rea";
        viewModel.NpmDisplayName = "REA";
        viewModel.NpmArgumentsJson = "mcp --stdio";

        await viewModel.InspectAsync();

        Assert.Contains("JSON", viewModel.ErrorMessage, StringComparison.Ordinal);
        Assert.Empty(installer.NpmInspected);
        Assert.False(viewModel.HasInspection);
    }

    [Fact]
    public async Task Https_git_preflight_and_install_preserve_ref_and_subdirectory()
    {
        var installer = new FakeInstaller();
        var viewModel = CreateViewModel(installer);
        viewModel.UseGitRepository = true;
        viewModel.GitRepositoryUrl = "https://example.test/owner/plugins.git";
        viewModel.GitReference = "refs/tags/v1.2.3";
        viewModel.GitSubdirectory = "plugins/fixture";

        Assert.True(viewModel.IsGitSource);
        Assert.False(viewModel.IsLocalSource);
        Assert.False(viewModel.IsNpmSource);
        Assert.True(viewModel.CanInspect);

        await viewModel.InspectAsync();

        var inspected = Assert.Single(installer.Inspected);
        Assert.Equal(LocalPluginSourceKind.GitRepository, inspected.Kind);
        Assert.Equal("https://example.test/owner/plugins.git", inspected.Location);
        Assert.Equal("refs/tags/v1.2.3", inspected.GitReference);
        Assert.Equal("plugins/fixture", inspected.Subdirectory);

        Assert.True(await viewModel.InstallAsync());
        var installed = Assert.Single(installer.Installed);
        Assert.Equal(inspected, installed.Source);
        Assert.Equal(installer.Inspection.Fingerprint, installed.Fingerprint);
    }

    [Fact]
    public void Git_and_npm_modes_are_mutually_exclusive_and_require_complete_source_identity()
    {
        var viewModel = CreateViewModel(new FakeInstaller());
        viewModel.UseNpmPackage = true;
        Assert.True(viewModel.IsNpmSource);

        viewModel.UseGitRepository = true;

        Assert.True(viewModel.IsGitSource);
        Assert.False(viewModel.UseNpmPackage);
        Assert.False(viewModel.CanInspect);
        viewModel.GitRepositoryUrl = "https://example.test/plugin.git";
        Assert.False(viewModel.CanInspect);
        viewModel.GitReference = "main";
        Assert.True(viewModel.CanInspect);
    }

    [Fact]
    public async Task Changing_source_mode_invalidates_the_local_preflight()
    {
        var installer = new FakeInstaller();
        var viewModel = CreateViewModel(installer);
        viewModel.SourcePath = @"C:\fixtures\plugin-one";
        await viewModel.InspectAsync();
        Assert.True(viewModel.HasInspection);

        viewModel.UseNpmPackage = true;

        Assert.False(viewModel.HasInspection);
        Assert.False(viewModel.CanInstall);
        Assert.True(viewModel.IsNpmSource);
        Assert.False(viewModel.IsLocalSource);
    }

    [Fact]
    public async Task Changing_npm_source_cancels_an_inflight_preflight_before_it_can_restore_stale_results()
    {
        var installer = new FakeInstaller { WaitForNpmCancellation = true };
        var viewModel = CreateViewModel(installer);
        viewModel.UseNpmPackage = true;
        viewModel.NpmPackageName = "rea-agents";
        viewModel.NpmPackageVersion = "6.5.0";
        viewModel.NpmBinaryName = "rea";
        viewModel.NpmComponentKey = "rea";
        viewModel.NpmDisplayName = "REA";

        var inspection = viewModel.InspectAsync();
        Assert.True(viewModel.IsBusy);

        viewModel.NpmPackageVersion = "6.5.1";
        await inspection;

        Assert.False(viewModel.IsBusy);
        Assert.False(viewModel.HasInspection);
        Assert.False(viewModel.CanInstall);
    }

    private static CustomPluginInstallerViewModel CreateViewModel(ILocalPluginSourceInstaller installer)
    {
        var dispatcher = new ImmediateUiDispatcher();
        var preferences = new AppPreferencesManager(new MemoryPreferencesStore());
        var localization = new LocalizationViewModel(preferences, dispatcher);
        return new CustomPluginInstallerViewModel(installer, localization, dispatcher);
    }

    private sealed class FakeInstaller : ILocalPluginSourceInstaller
    {
        public LocalPluginInspection Inspection { get; } = new(
            new string('a', 64),
            "Fixture",
            "1.0.0",
            @"Local folder · C:\fixtures\plugin-one",
            [new LocalPluginInspectionComponent(
                "fixture",
                "Fixture MCP",
                LocalPluginInspectionComponentKind.Mcp,
                ["ping", "status"])]);

        public Exception? InspectError { get; init; }
        public bool WaitForCancellation { get; set; }
        public bool WaitForNpmCancellation { get; set; }
        public List<LocalPluginSourceDraft> Inspected { get; } = [];
        public List<(LocalPluginSourceDraft Source, string Fingerprint)> Installed { get; } = [];
        public List<NpmMcpDraft> NpmInspected { get; } = [];
        public List<(NpmMcpDraft Draft, string Fingerprint)> NpmInstalled { get; } = [];

        public async Task<LocalPluginInspection> InspectAsync(
            LocalPluginSourceDraft source,
            CancellationToken cancellationToken = default)
        {
            Inspected.Add(source);
            if (WaitForCancellation)
            {
                await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
            }
            if (InspectError is not null) throw InspectError;
            return Inspection;
        }

        public Task<InstalledPluginRecord> InstallAsync(
            LocalPluginSourceDraft source,
            string expectedFingerprint,
            CancellationToken cancellationToken = default)
        {
            Installed.Add((source, expectedFingerprint));
            return Task.FromResult(new InstalledPluginRecord(
                "custom-fixture",
                "custom-release",
                "1.0.0",
                new string('b', 64),
                @"C:\installed\custom-fixture",
                DateTimeOffset.UtcNow,
                [],
                new Dictionary<string, string>(),
                "fixture"));
        }

        public async Task<LocalPluginInspection> InspectNpmMcpAsync(
            NpmMcpDraft draft,
            CancellationToken cancellationToken = default)
        {
            NpmInspected.Add(draft);
            if (WaitForNpmCancellation)
            {
                await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
            }
            return Inspection;
        }

        public Task<InstalledPluginRecord> InstallNpmMcpAsync(
            NpmMcpDraft draft,
            string expectedFingerprint,
            CancellationToken cancellationToken = default)
        {
            NpmInstalled.Add((draft, expectedFingerprint));
            return Task.FromResult(new InstalledPluginRecord(
                "custom-npm-fixture",
                "custom-npm-release",
                draft.PackageVersion,
                new string('c', 64),
                @"C:\installed\custom-npm-fixture",
                DateTimeOffset.UtcNow,
                ["process.spawn"],
                new Dictionary<string, string>(),
                draft.PackageName));
        }
    }

    private sealed class MemoryPreferencesStore : IAppPreferencesStore
    {
        public Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<AppPreferences?>(AppPreferences.Default);

        public Task SaveAsync(
            AppPreferences preferences,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
    }
}
