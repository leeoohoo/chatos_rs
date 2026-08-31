using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IPetActivityInboxService
{
    Task<IReadOnlyList<PetActivity>> FetchOpenActivitiesAsync(
        int limit = 100,
        CancellationToken cancellationToken = default);

    Task ApplyAsync(
        PetActivityDisposition disposition,
        PetActivity activity,
        CancellationToken cancellationToken = default);
}
