using System.Text.Json;
using ChatOS.Api.Workspace;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Tests;

public sealed class RemoteConnectionCloudServiceTests
{
    [Fact]
    public async Task ListMapsAuthenticationJumpAndCredentialFlags()
    {
        var service = Service(request =>
        {
            Assert.Equal("/api/chatos/remote-connections", request.RequestUri?.AbsolutePath);
            return StubHttpMessageHandler.Json("""
                [{
                  "id":"remote-1","name":"Server","host":"10.0.0.2","port":2222,
                  "username":"deploy","auth_type":"private_key_cert","has_private_key_path":true,
                  "has_certificate_path":true,"host_key_policy":"accept_new",
                  "local_connector_device_id":"device-1","local_connector_workspace_id":"workspace-1",
                  "jump_enabled":true,"jump_connection_id":"jump-1","has_jump_password":true
                }]
                """);
        });

        var connection = Assert.Single(await service.ListAsync());

        Assert.Equal(RemoteAuthenticationType.PrivateKeyCertificate, connection.AuthenticationType);
        Assert.Equal(RemoteHostKeyPolicy.AcceptNew, connection.HostKeyPolicy);
        Assert.True(connection.HasPrivateKeyPath);
        Assert.True(connection.HasCertificatePath);
        Assert.True(connection.JumpEnabled);
        Assert.True(connection.HasJumpPassword);
    }

    [Fact]
    public async Task CreateNeverUploadsLocalSecretsOrCredentialPaths()
    {
        var service = Service(request =>
        {
            Assert.Equal(HttpMethod.Post, request.Method);
            var json = request.Content!.ReadAsStringAsync().GetAwaiter().GetResult();
            using var document = JsonDocument.Parse(json);
            var root = document.RootElement;
            Assert.False(root.TryGetProperty("password", out _));
            Assert.False(root.TryGetProperty("private_key_path", out _));
            Assert.False(root.TryGetProperty("certificate_path", out _));
            Assert.False(root.TryGetProperty("jump_password", out _));
            Assert.Equal("server.example", root.GetProperty("host").GetString());
            return StubHttpMessageHandler.Json("""
                {"id":"remote-1","name":"Server","host":"server.example","port":22,
                 "username":"deploy","auth_type":"password","host_key_policy":"strict",
                 "local_connector_device_id":"device-1","local_connector_workspace_id":"workspace-1"}
                """);
        });

        var created = await service.CreateAsync(Draft());

        Assert.Equal("remote-1", created.Id);
        Assert.Equal(RemoteAuthenticationType.Password, created.AuthenticationType);
    }

    [Fact]
    public async Task UpdateAndDeleteEncodeConnectionIdentifierAsSinglePathSegment()
    {
        var calls = new List<(HttpMethod Method, string Path)>();
        var service = Service(request =>
        {
            calls.Add((request.Method, request.RequestUri!.AbsolutePath));
            return request.Method == HttpMethod.Delete
                ? StubHttpMessageHandler.Json("{\"success\":true}")
                : StubHttpMessageHandler.Json("""
                    {"id":"remote/a","name":"Server","host":"server.example","port":22,
                     "username":"deploy","auth_type":"password","host_key_policy":"strict",
                     "local_connector_device_id":"device-1","local_connector_workspace_id":"workspace-1"}
                    """);
        });

        await service.UpdateAsync("remote/a", Draft());
        await service.DeleteAsync("remote/a");

        Assert.All(calls, call => Assert.Equal("/api/chatos/remote-connections/remote%2Fa", call.Path));
    }

    private static RemoteConnectionCloudService Service(
        Func<HttpRequestMessage, HttpResponseMessage> handler) =>
        new(ApiTestClient.Create(new MemoryTokenStore(), handler));

    private static RemoteConnectionDraft Draft() => new(
        " Server ",
        " server.example ",
        22,
        " deploy ",
        RemoteAuthenticationType.Password,
        "super-secret",
        "C:\\keys\\id_ed25519",
        "C:\\keys\\id_ed25519-cert.pub",
        "/srv/app",
        RemoteHostKeyPolicy.Strict,
        "device-1",
        "workspace-1",
        false,
        null,
        null,
        null,
        null,
        null,
        null,
        "jump-secret");
}
