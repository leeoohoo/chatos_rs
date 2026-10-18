using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

public sealed record WindowsLocalAgentArtifactItem(
    string ArtifactId,
    string Name,
    string MimeType,
    ulong Size,
    string Sha256,
    string Status,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs);

public sealed record WindowsLocalAgentArtifactPage(
    IReadOnlyList<WindowsLocalAgentArtifactItem> Artifacts,
    string? NextCursor);

public sealed class WindowsLocalAgentArtifactClient(ILocalAgentHostClient host)
{
    public async Task<WindowsLocalAgentArtifactPage> ListAsync(
        string ownerUserId,
        int limit = 50,
        string? cursor = null,
        CancellationToken cancellationToken = default)
    {
        var result = await host.SendAsync<ListArtifactsCommand, ArtifactsResult>(
            new("list_artifacts", ownerUserId, (uint)Math.Clamp(limit, 1, 500), cursor),
            cancellationToken).ConfigureAwait(false);
        return string.Equals(result.Type, "artifacts", StringComparison.Ordinal)
            ? result.Page
            : throw new InvalidDataException(
                "Local Agent Host returned invalid artifact list data.");
    }

    public async Task<byte[]> ReadAsync(
        string ownerUserId,
        string artifactId,
        CancellationToken cancellationToken = default)
    {
        var result = await host.SendAsync<ArtifactIdentityCommand, ArtifactDataResult>(
            new("get_artifact_data", ownerUserId, artifactId),
            cancellationToken).ConfigureAwait(false);
        if (!string.Equals(result.Type, "artifact_data", StringComparison.Ordinal) ||
            !string.Equals(result.ArtifactId, artifactId, StringComparison.Ordinal))
        {
            throw new InvalidDataException(
                "Local Agent Host returned invalid artifact data.");
        }

        try
        {
            return Convert.FromBase64String(result.DataBase64);
        }
        catch (FormatException exception)
        {
            throw new InvalidDataException(
                "Local Agent Host returned malformed artifact data.", exception);
        }
    }

    private sealed record ListArtifactsCommand(
        string Type,
        string OwnerUserId,
        uint Limit,
        string? Cursor);

    private sealed record ArtifactIdentityCommand(
        string Type,
        string OwnerUserId,
        string ArtifactId);

    private sealed record ArtifactsResult(
        string Type,
        WindowsLocalAgentArtifactPage Page);

    private sealed record ArtifactDataResult(
        string Type,
        string ArtifactId,
        string DataBase64);
}
