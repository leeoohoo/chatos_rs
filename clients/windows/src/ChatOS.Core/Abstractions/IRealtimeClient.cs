using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IRealtimeClient
{
    IAsyncEnumerable<PetActivityEvent> StreamPetActivitiesAsync(
        CancellationToken cancellationToken = default);
}
