using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IClipboardHistoryStore
{
    Task<ClipboardHistoryEntry> StoreAsync(
        ClipboardHistoryPayload payload,
        string? sourceApplication,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<ClipboardHistoryEntry>> ListAsync(
        int limit = 500,
        CancellationToken cancellationToken = default);

    Task<ClipboardHistoryPayload?> ReadPayloadAsync(
        Guid id,
        CancellationToken cancellationToken = default);

    Task SetPinnedAsync(Guid id, bool pinned, CancellationToken cancellationToken = default);

    Task DeleteAsync(Guid id, CancellationToken cancellationToken = default);

    Task PruneAsync(DateTimeOffset cutoff, int unpinnedLimit = 500, CancellationToken cancellationToken = default);
}
