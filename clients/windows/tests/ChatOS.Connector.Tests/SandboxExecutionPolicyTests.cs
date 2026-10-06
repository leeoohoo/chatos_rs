using ChatOS.Connector.Sandbox;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.Tests;

public sealed class SandboxExecutionPolicyTests
{
    [Fact]
    public void ControlledNetworkRemainsExplicitAndCannotSilentlyBroadenOrDowngrade()
    {
        var settings = new ConnectorSandboxSettings(
            true,
            ConnectorSandboxPermissionProfile.WorkspaceWrite,
            ConnectorSandboxNetworkAccess.Controlled);

        var policy = SandboxExecutionPolicy.FromSettings(settings);

        Assert.True(policy.UseAppContainer);
        Assert.Equal(ConnectorSandboxNetworkAccess.Controlled, policy.NetworkAccess);
        Assert.True(policy.AllowHostNetwork);
        Assert.False(policy.GrantInternetCapabilities);
    }

    [Fact]
    public void DefaultPolicyUsesCompatibleHostAccess()
    {
        var policy = SandboxExecutionPolicy.FromSettings(ConnectorSandboxSettings.Default);

        Assert.False(policy.UseAppContainer);
        Assert.Equal(ConnectorSandboxPermissionProfile.FullAccess, policy.PermissionProfile);
        Assert.True(policy.AllowHostNetwork);
    }

    [Fact]
    public void RestrictedEnvironmentUsesAllowlistInsteadOfInheritingProcessSecrets()
    {
        var policy = SandboxExecutionPolicy.FromSettings(new ConnectorSandboxSettings(
            true,
            ConnectorSandboxPermissionProfile.WorkspaceWrite,
            ConnectorSandboxNetworkAccess.Disabled));
        var environment = WindowsAppContainerLaunchContext.BuildEnvironmentVariables(
            "C:\\sandbox-temp",
            policy);

        Assert.Equal("1", environment["CHATOS_SANDBOX"]);
        Assert.Equal("C:\\sandbox-temp", environment["TEMP"]);
        Assert.Equal("C:\\sandbox-temp", environment["LOCALAPPDATA"]);
        Assert.DoesNotContain("OPENAI_API_KEY", environment.Keys, StringComparer.OrdinalIgnoreCase);
        Assert.DoesNotContain("CHATOS_API_TOKEN", environment.Keys, StringComparer.OrdinalIgnoreCase);
        Assert.DoesNotContain("USERPROFILE", environment.Keys, StringComparer.OrdinalIgnoreCase);
    }

    [Fact]
    public void FullAccessCannotPretendNetworkIsDisabled()
    {
        var policy = SandboxExecutionPolicy.FromSettings(new ConnectorSandboxSettings(
            true,
            ConnectorSandboxPermissionProfile.FullAccess,
            ConnectorSandboxNetworkAccess.Disabled));

        Assert.False(policy.UseAppContainer);
        Assert.Equal(ConnectorSandboxNetworkAccess.Host, policy.NetworkAccess);
    }

    [Fact]
    public void AppContainerProfilesAreStableButIsolatedPerWorkspaceAndPermission()
    {
        var firstRoot = Path.Combine(Path.GetTempPath(), "chatos-profile-a");
        var secondRoot = Path.Combine(Path.GetTempPath(), "chatos-profile-b");

        var firstWrite = WindowsAppContainerSandbox.ProfileName(
            firstRoot,
            ConnectorSandboxPermissionProfile.WorkspaceWrite);
        var repeatedWrite = WindowsAppContainerSandbox.ProfileName(
            firstRoot + Path.DirectorySeparatorChar,
            ConnectorSandboxPermissionProfile.WorkspaceWrite);
        var firstRead = WindowsAppContainerSandbox.ProfileName(
            firstRoot,
            ConnectorSandboxPermissionProfile.ReadOnly);
        var secondWrite = WindowsAppContainerSandbox.ProfileName(
            secondRoot,
            ConnectorSandboxPermissionProfile.WorkspaceWrite);

        Assert.Equal(firstWrite, repeatedWrite);
        Assert.NotEqual(firstWrite, firstRead);
        Assert.NotEqual(firstWrite, secondWrite);
        Assert.StartsWith("ChatOS.Sandbox.w.v2.", firstWrite, StringComparison.Ordinal);
        Assert.Equal(52, firstWrite.Length);
    }

    [Fact]
    public void ControlledProfilesAreIsolatedPerSignedPolicyRevision()
    {
        var root = Path.Combine(Path.GetTempPath(), "chatos-controlled-profile");

        var first = WindowsAppContainerSandbox.ProfileName(
            root,
            ConnectorSandboxPermissionProfile.WorkspaceWrite,
            "policy-revision-1");
        var repeated = WindowsAppContainerSandbox.ProfileName(
            root,
            ConnectorSandboxPermissionProfile.WorkspaceWrite,
            "policy-revision-1");
        var second = WindowsAppContainerSandbox.ProfileName(
            root,
            ConnectorSandboxPermissionProfile.WorkspaceWrite,
            "policy-revision-2");

        Assert.Equal(first, repeated);
        Assert.NotEqual(first, second);
        Assert.Contains(".controlled.v1.", first, StringComparison.Ordinal);
        Assert.True(first.Length <= 64);
    }

    [Fact]
    public void PluginWorkspaceWriteNeverMakesTheInstallationWritable()
    {
        var record = new InstalledPluginRecord(
            "plugin-1",
            "release-1",
            "1.0.0",
            new string('a', 64),
            "C:\\Plugin",
            DateTimeOffset.UtcNow,
            ["process.spawn", "workspace.write"]);
        var request = new PluginProcessLaunchRequest(
            record,
            "workbench",
            "C:\\Plugin\\plugin.exe",
            [],
            new Dictionary<string, string>(),
            "C:\\Plugin",
            "C:\\Workspace",
            new HashSet<string>(record.DeclaredPermissions, StringComparer.Ordinal),
            ConnectorSandboxNetworkAccess.Loopback);

        var installation = WindowsPluginProcessLauncher.InstallationPolicy(request);
        var workspace = WindowsPluginProcessLauncher.WorkspacePermissionProfile(
            request.PermissionSnapshot);

        Assert.Equal(ConnectorSandboxPermissionProfile.ReadOnly, installation.PermissionProfile);
        Assert.Equal(ConnectorSandboxNetworkAccess.Loopback, installation.NetworkAccess);
        Assert.False(installation.GrantInternetCapabilities);
        Assert.Equal(ConnectorSandboxPermissionProfile.WorkspaceWrite, workspace);
    }
}
