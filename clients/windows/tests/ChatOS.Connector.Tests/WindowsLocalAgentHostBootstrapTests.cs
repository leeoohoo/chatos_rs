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
            var credentials = new FakeCredentials();
            var builder = new WindowsLocalAgentHostBootstrapBuilder(credentials);
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
                Convert.ToBase64String(Enumerable.Repeat((byte)7, 32).ToArray()),
                request.GetProperty("credentials").GetProperty("storage")
                    .GetProperty("encryption_key_base64").GetString());
            Assert.Equal(
                "model-token",
                request.GetProperty("credentials").GetProperty("model_access_token").GetString());
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
    public async Task BuildsOnlyVerifyFullPostgresAndRedactsItsDisplay()
    {
        var root = Path.Combine(Path.GetTempPath(), $"chatos-bootstrap-{Guid.NewGuid():N}");
        try
        {
            var credentials = new FakeCredentials();
            var postgres = new WindowsLocalAgentPostgresCredential(
                "database.example.com",
                5432,
                "chatos",
                "verify_full",
                "chatos-user",
                "private-password");
            credentials.Postgres = JsonSerializer.Serialize(postgres);
            var configuration = await new WindowsLocalAgentHostBootstrapBuilder(credentials)
                .BuildAsync(Settings(root, new WindowsLocalAgentPostgresBootstrap("postgres-1")));
            using var document = JsonDocument.Parse(configuration.LaunchMaterial.RequestJson);
            var storage = document.RootElement.GetProperty("credentials").GetProperty("storage");

            Assert.Equal("verify_full", storage.GetProperty("tls_mode").GetString());
            Assert.Equal("private-password", storage.GetProperty("password").GetString());
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
        ModelGatewayBaseUri = new Uri("https://api.example.com"),
        MemoryEngineBaseUri = new Uri("https://memory.example.com"),
        Storage = storage,
    };

    private sealed class FakeCredentials : IWindowsLocalAgentCredentialStore
    {
        public string Postgres { get; set; } = "{}";

        public ValueTask<string?> LoadCredentialAsync(
            string accountId,
            string reference,
            CancellationToken cancellationToken = default) =>
            ValueTask.FromResult<string?>(reference switch
            {
                WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference => "model-token",
                "postgres-1" => Postgres,
                _ => null,
            });

        public Task<byte[]?> LoadDeviceKeyAsync(
            string accountId,
            string reference,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<byte[]?>(reference switch
            {
                WindowsLocalAgentHostBootstrapBuilder.ProviderContextKeyReference =>
                    Enumerable.Repeat((byte)3, 32).ToArray(),
                "sqlite-key" => Enumerable.Repeat((byte)7, 32).ToArray(),
                _ => null,
            });
    }
}
