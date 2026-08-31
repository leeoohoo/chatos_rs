using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IPetActivitySuppressionStore
{
    Task<bool> IsSuppressedAsync(
        string stableIdentity,
        DateTimeOffset now,
        CancellationToken cancellationToken = default);

    Task SuppressAsync(
        string stableIdentity,
        PetActivityDisposition disposition,
        DateTimeOffset suppressedAt,
        DateTimeOffset? expiresAt,
        CancellationToken cancellationToken = default);

    Task RemoveAsync(
        string stableIdentity,
        CancellationToken cancellationToken = default);

    Task PruneExpiredAsync(
        DateTimeOffset now,
        CancellationToken cancellationToken = default);
}
