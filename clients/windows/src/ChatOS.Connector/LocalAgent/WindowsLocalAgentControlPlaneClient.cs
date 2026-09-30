using System.Text.Json;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

public sealed record WindowsLocalAgentModelSnapshot(
    string OwnerUserId,
    string ModelConfigRef,
    string ModelConfigRevision,
    string CredentialRef,
    string BaseUrl,
    string Model,
    string Provider,
    bool SupportsResponses,
    bool? SupportsImages,
    string? Instructions,
    double? Temperature,
    long? MaxOutputTokens,
    string? ThinkingLevel,
    bool IncludePromptCacheRetention,
    ulong? RequestBodyLimitBytes,
    uint? MaxTransientRetries,
    JsonElement? OutputFormat);

public sealed record WindowsLocalAgentCapabilitySnapshot(
    string OwnerUserId,
    string ProfileKey,
    string CapabilityPolicyRevision,
    string? Instructions,
    IReadOnlyList<JsonElement> PrefixedInputItems,
    IReadOnlyList<JsonElement> Tools);

public sealed class WindowsLocalAgentControlPlaneClient(ILocalAgentHostClient host)
{
    public async Task<WindowsLocalAgentModelSnapshot> PublishModelAsync(
        WindowsLocalAgentModelSnapshot snapshot,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<PutModelCommand, ModelResult>(
            new("put_model_config_snapshot", snapshot),
            cancellationToken).ConfigureAwait(false);
        return response.Type == "model_config_snapshot"
            ? response.Snapshot
            : throw new InvalidDataException("Local Agent Host returned an invalid model snapshot result.");
    }

    public async Task<WindowsLocalAgentCapabilitySnapshot> PublishCapabilitiesAsync(
        WindowsLocalAgentCapabilitySnapshot snapshot,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<PutCapabilityCommand, CapabilityResult>(
            new("put_capability_policy_snapshot", snapshot),
            cancellationToken).ConfigureAwait(false);
        return response.Type == "capability_policy_snapshot"
            ? response.Snapshot
            : throw new InvalidDataException("Local Agent Host returned an invalid capability snapshot result.");
    }

    private sealed record PutModelCommand(
        string Type,
        WindowsLocalAgentModelSnapshot Snapshot);

    private sealed record PutCapabilityCommand(
        string Type,
        WindowsLocalAgentCapabilitySnapshot Snapshot);

    private sealed record ModelResult(
        string Type,
        WindowsLocalAgentModelSnapshot Snapshot);

    private sealed record CapabilityResult(
        string Type,
        WindowsLocalAgentCapabilitySnapshot Snapshot);
}
