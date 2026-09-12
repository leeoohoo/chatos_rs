using System.Text.Json;
using ChatOS.Connector.LocalAgent;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentHostBootstrapTests
{
    [Fact]
    public async Task BuildsExactSqliteLaunchContractFromSecureReferences()
    {
        var root = Path.Combine(Path.GetTempPath(), $"chatos-bootstrap-{Guid.NewGuid():N}");
        try
        {
            var builder = new WindowsLocalAgentHostBootstrapBuilder();
            var configuration = await builder.BuildAsync(Settings(
                root,
                new WindowsLocalAgentSqliteBootstrap(
                    Path.Combine(root, "client.sqlite"),
                    "sqlite-key")));
            using var document = JsonDocument.Parse(configuration.LaunchMaterial.RequestJson);
            var request = document.RootElement;

            Assert.Equal("user-1", request.GetProperty("owner_user_id").GetString());
            Assert.Equal(
                "windows_named_pipe",
                request.GetProperty("ipc_endpoint").GetProperty("transport").GetString());
            Assert.Equal("sqlite", request.GetProperty("storage_profile").GetProperty("backend").GetString());
            Assert.Equal(
                "model-access-token",
                request.GetProperty("credential_references")
                    .GetProperty("model_access_token_reference").GetString());
            Assert.Equal(
                "provider-context-key",
                request.GetProperty("credential_references")
                    .GetProperty("provider_context_key_reference").GetString());
            Assert.False(request.TryGetProperty("credentials", out _));
            configuration.LaunchMaterial.Dispose();
        }
        finally
        {
            if (Directory.Exists(root))
            {
                Directory.Delete(root, recursive: true);
            }
        }
    }

    [Fact]
    public async Task PassesOnlyPostgresReferenceAndRedactsCredentialDisplay()
    {
        var root = Path.Combine(Path.GetTempPath(), $"chatos-bootstrap-{Guid.NewGuid():N}");
        try
        {
            var postgres = new WindowsLocalAgentPostgresCredential(
                "database.example.com",
                5432,
                "chatos",
                "verify_full",
                "chatos-user",
                "private-password");
            var configuration = await new WindowsLocalAgentHostBootstrapBuilder()
                .BuildAsync(Settings(root, new WindowsLocalAgentPostgresBootstrap("postgres-1")));
            using var document = JsonDocument.Parse(configuration.LaunchMaterial.RequestJson);
            var storage = document.RootElement.GetProperty("storage_profile");

            Assert.Equal("postgres-1", storage.GetProperty("connection_secret").GetString());
            Assert.False(document.RootElement.TryGetProperty("credentials", out _));
            Assert.DoesNotContain(
                "private-password",
                System.Text.Encoding.UTF8.GetString(configuration.LaunchMaterial.RequestJson.Span));
            Assert.DoesNotContain("private-password", postgres.ToString(), StringComparison.Ordinal);
            Assert.DoesNotContain("database.example.com", postgres.ToString(), StringComparison.Ordinal);
            configuration.LaunchMaterial.Dispose();
        }
        finally
        {
            if (Directory.Exists(root))
            {
                Directory.Delete(root, recursive: true);
            }
        }
    }

    private static WindowsLocalAgentHostBootstrapSettings Settings(
        string root,
        WindowsLocalAgentStorageBootstrap storage) => new()
    {
        ExecutablePath = Path.Combine(root, "local-agent-host.exe"),
        ExpectedExecutableSha256 = $"sha256:{new string('0', 64)}",
        AccountId = "user-1",
        DeviceId = "device-1",
        AttachmentGrantDirectory = Path.Combine(root, "grants"),
        PlatformStateDirectory = Path.Combine(root, "state"),
        ModelGatewayBaseUri = new Uri("https://api.example.com"),
        MemoryEngineBaseUri = new Uri("https://memory.example.com"),
        Storage = storage,
    };
}
