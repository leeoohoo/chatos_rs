using System.Security.Cryptography;
using System.Text;
using ChatOS.Connector.LocalAgent;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentRuntimeConfigurationTests
{
    [Fact]
    public void CreatesAccountScopedSqliteSettingsFromTypedConfiguration()
    {
        var root = Path.Combine(Path.GetTempPath(), $"chatos-runtime-{Guid.NewGuid():N}");
        var executable = Path.Combine(root, "package", "chatos_local_agent_host.exe");
        var configuration = new WindowsLocalAgentRuntimeConfiguration(
            new WindowsLocalAgentRuntimeOptions
            {
                HostExecutablePath = executable,
                HostExecutableSha256 = $"sha256:{new string('a', 64)}",
                StateRootDirectory = root,
            },
            "https://services.example.test/api/chatos/");

        var settings = configuration.Create("user-1", "device-0123456789abcdef0123456789abcdef");

        var accountHash = Convert.ToHexString(
            SHA256.HashData(Encoding.UTF8.GetBytes("user-1"))).ToLowerInvariant();
        Assert.Equal(Path.GetFullPath(executable), settings.ExecutablePath);
        Assert.Equal(new Uri("https://services.example.test/"), settings.ModelGatewayBaseUri);
        Assert.Equal(new Uri("https://services.example.test/"), settings.MemoryEngineBaseUri);
        Assert.Contains(Path.Combine("Accounts", accountHash), settings.PlatformStateDirectory);
        var sqlite = Assert.IsType<WindowsLocalAgentSqliteBootstrap>(settings.Storage);
        Assert.Equal(WindowsLocalAgentAccountSession.SqliteEncryptionKeyReference,
            sqlite.EncryptionSecretReference);
        Assert.Contains(Path.Combine("Accounts", accountHash), sqlite.DatabasePath);
    }

    [Fact]
    public void RefusesToTrustAHostWithoutAPackagedDigest()
    {
        var configuration = new WindowsLocalAgentRuntimeConfiguration(
            new WindowsLocalAgentRuntimeOptions
            {
                HostExecutablePath = Path.Combine(Path.GetTempPath(), "local-agent-host.exe"),
                StateRootDirectory = Path.GetTempPath(),
            },
            "https://services.example.test/api/chatos/");

        var error = Assert.Throws<InvalidOperationException>(() =>
            configuration.Create("user-1", "device-0123456789abcdef0123456789abcdef"));

        Assert.Contains("SHA-256", error.Message, StringComparison.Ordinal);
    }
}
