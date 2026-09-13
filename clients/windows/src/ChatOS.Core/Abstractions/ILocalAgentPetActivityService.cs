using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

/// <summary>
/// Account-scoped Pet projection over the local Agent authority. Implementations
/// must not fetch, acknowledge, or mutate a server-side activity inbox.
/// </summary>
public interface ILocalAgentPetActivityService
{
    event EventHandler? Changed;

    Task<IReadOnlyList<PetActivity>> FetchAsync(
        CancellationToken cancellationToken = default);

    Task SuppressAsync(
        PetActivity activity,
        PetActivityDisposition disposition,
        CancellationToken cancellationToken = default);
}
