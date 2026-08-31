using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Workspace;

public sealed class RemoteConnectionCloudService : IRemoteConnectionCloudService
{
    private readonly ChatOSApiClient _client;

    public RemoteConnectionCloudService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<IReadOnlyList<RemoteConnection>> ListAsync(
        CancellationToken cancellationToken = default)
    {
        var response = await _client.GetAsync<IReadOnlyList<RemoteConnectionDto>>(
            "remote-connections",
            cancellationToken).ConfigureAwait(false);
        return response.Select(static value => value.ToDomain()).ToArray();
    }

    public async Task<RemoteConnection> CreateAsync(
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.PostAsync<RemoteConnectionDto>(
            "remote-connections",
            RemoteConnectionDraftDto.From(draft),
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain();
    }

    public async Task<RemoteConnection> UpdateAsync(
        string id,
        RemoteConnectionDraft draft,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.PutAsync<RemoteConnectionDto>(
            $"remote-connections/{Uri.EscapeDataString(id)}",
            RemoteConnectionDraftDto.From(draft),
            cancellationToken).ConfigureAwait(false);
        return response.ToDomain();
    }

    public async Task DeleteAsync(string id, CancellationToken cancellationToken = default)
    {
        _ = await _client.DeleteAsync<RemoteConnectionMutationDto>(
            $"remote-connections/{Uri.EscapeDataString(id)}",
            cancellationToken).ConfigureAwait(false);
    }
}

internal sealed record RemoteConnectionDto
{
    [JsonPropertyName("id")] public required string Id { get; init; }
    [JsonPropertyName("name")] public string? Name { get; init; }
    [JsonPropertyName("host")] public string? Host { get; init; }
    [JsonPropertyName("port")] public int? Port { get; init; }
    [JsonPropertyName("username")] public string? Username { get; init; }
    [JsonPropertyName("auth_type")] public string? AuthenticationType { get; init; }
    [JsonPropertyName("has_password")] public bool? HasPassword { get; init; }
    [JsonPropertyName("has_private_key_path")] public bool? HasPrivateKeyPath { get; init; }
    [JsonPropertyName("has_certificate_path")] public bool? HasCertificatePath { get; init; }
    [JsonPropertyName("default_remote_path")] public string? DefaultRemotePath { get; init; }
    [JsonPropertyName("host_key_policy")] public string? HostKeyPolicy { get; init; }
    [JsonPropertyName("local_connector_device_id")] public string? LocalConnectorDeviceId { get; init; }
    [JsonPropertyName("local_connector_workspace_id")] public string? LocalConnectorWorkspaceId { get; init; }
    [JsonPropertyName("jump_enabled")] public bool? JumpEnabled { get; init; }
    [JsonPropertyName("jump_connection_id")] public string? JumpConnectionId { get; init; }
    [JsonPropertyName("jump_host")] public string? JumpHost { get; init; }
    [JsonPropertyName("jump_port")] public int? JumpPort { get; init; }
    [JsonPropertyName("jump_username")] public string? JumpUsername { get; init; }
    [JsonPropertyName("has_jump_private_key_path")] public bool? HasJumpPrivateKeyPath { get; init; }
    [JsonPropertyName("has_jump_certificate_path")] public bool? HasJumpCertificatePath { get; init; }
    [JsonPropertyName("has_jump_password")] public bool? HasJumpPassword { get; init; }
    [JsonPropertyName("last_active_at")] public DateTimeOffset? LastActiveAt { get; init; }

    public RemoteConnection ToDomain() => new(
        Id,
        Clean(Name) ?? "未命名远端",
        Host ?? string.Empty,
        Port ?? 22,
        Username ?? string.Empty,
        AuthenticationType switch
        {
            "password" => RemoteAuthenticationType.Password,
            "private_key_cert" => RemoteAuthenticationType.PrivateKeyCertificate,
            _ => RemoteAuthenticationType.PrivateKey,
        },
        HasPassword ?? false,
        HasPrivateKeyPath ?? false,
        HasCertificatePath ?? false,
        Clean(DefaultRemotePath),
        HostKeyPolicy == "accept_new" ? RemoteHostKeyPolicy.AcceptNew : RemoteHostKeyPolicy.Strict,
        LocalConnectorDeviceId ?? string.Empty,
        LocalConnectorWorkspaceId ?? string.Empty,
        JumpEnabled ?? false,
        Clean(JumpConnectionId),
        Clean(JumpHost),
        JumpPort,
        Clean(JumpUsername),
        HasJumpPrivateKeyPath ?? false,
        HasJumpCertificatePath ?? false,
        HasJumpPassword ?? false,
        LastActiveAt);

    private static string? Clean(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();
}

internal sealed record RemoteConnectionDraftDto(
    [property: JsonPropertyName("name")] string? Name,
    [property: JsonPropertyName("host")] string Host,
    [property: JsonPropertyName("port")] int Port,
    [property: JsonPropertyName("username")] string Username,
    [property: JsonPropertyName("auth_type")] string AuthenticationType,
    [property: JsonPropertyName("default_remote_path")] string? DefaultRemotePath,
    [property: JsonPropertyName("host_key_policy")] string HostKeyPolicy,
    [property: JsonPropertyName("local_connector_device_id")] string LocalConnectorDeviceId,
    [property: JsonPropertyName("local_connector_workspace_id")] string LocalConnectorWorkspaceId,
    [property: JsonPropertyName("jump_enabled")] bool JumpEnabled,
    [property: JsonPropertyName("jump_connection_id")] string? JumpConnectionId,
    [property: JsonPropertyName("jump_host")] string? JumpHost,
    [property: JsonPropertyName("jump_port")] int? JumpPort,
    [property: JsonPropertyName("jump_username")] string? JumpUsername)
{
    public static RemoteConnectionDraftDto From(RemoteConnectionDraft draft) => new(
        Clean(draft.Name),
        draft.Host.Trim(),
        draft.Port,
        draft.Username.Trim(),
        draft.AuthenticationType switch
        {
            RemoteAuthenticationType.Password => "password",
            RemoteAuthenticationType.PrivateKeyCertificate => "private_key_cert",
            _ => "private_key",
        },
        Clean(draft.DefaultRemotePath),
        draft.HostKeyPolicy == RemoteHostKeyPolicy.AcceptNew ? "accept_new" : "strict",
        draft.LocalConnectorDeviceId,
        draft.LocalConnectorWorkspaceId,
        draft.JumpEnabled,
        Clean(draft.JumpConnectionId),
        Clean(draft.JumpHost),
        draft.JumpPort,
        Clean(draft.JumpUsername));

    private static string? Clean(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();
}

internal sealed record RemoteConnectionMutationDto(
    [property: JsonPropertyName("success")] bool? Success);
