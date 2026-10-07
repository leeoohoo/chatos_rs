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

    public async Task<IReadOnlyList<WindowsLocalAgentModelSnapshot>> LatestModelsAsync(
        string ownerUserId,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<ListLatestModelsCommand, ModelsResult>(
            new("list_latest_model_config_snapshots", ownerUserId),
            cancellationToken).ConfigureAwait(false);
        return response.Type == "model_config_snapshots"
            ? response.Snapshots
            : throw new InvalidDataException("Local Agent Host returned an invalid model snapshots result.");
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

    public async Task<WindowsLocalAgentCapabilitySnapshot> LatestCapabilitiesAsync(
        string ownerUserId,
        string profileKey,
        CancellationToken cancellationToken = default)
    {
        var response = await host.SendAsync<GetLatestCapabilityCommand, CapabilityResult>(
            new("get_latest_capability_policy_snapshot", ownerUserId, profileKey),
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

    private sealed record ListLatestModelsCommand(
        string Type,
        string OwnerUserId);

    private sealed record GetLatestCapabilityCommand(
        string Type,
        string OwnerUserId,
        string ProfileKey);

    private sealed record ModelResult(
        string Type,
        WindowsLocalAgentModelSnapshot Snapshot);

    private sealed record ModelsResult(
        string Type,
        IReadOnlyList<WindowsLocalAgentModelSnapshot> Snapshots);

    private sealed record CapabilityResult(
        string Type,
        WindowsLocalAgentCapabilitySnapshot Snapshot);
}
