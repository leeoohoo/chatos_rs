using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Pet;

public sealed class PetActivityInboxService : IPetActivityInboxService
{
    private readonly ChatOSApiClient _client;

    public PetActivityInboxService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<IReadOnlyList<PetActivity>> FetchOpenActivitiesAsync(
        int limit = 100,
        CancellationToken cancellationToken = default)
    {
        var normalizedLimit = Math.Clamp(limit, 1, 500);
        var response = await _client.GetAsync<PetActivityInboxListDto>(
            $"pet-activities?include_closed=false&mark_displayed=true&limit={normalizedLimit}",
            cancellationToken).ConfigureAwait(false);

        return response.Activities?
            .Select(activity => activity.ToDomain())
            .OfType<PetActivity>()
            .ToArray()
            ?? [];
    }

    public async Task ApplyAsync(
        PetActivityDisposition disposition,
        PetActivity activity,
        CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(activity.InboxId))
        {
            return;
        }

        var action = disposition switch
        {
            PetActivityDisposition.Acknowledged => "acknowledge",
            PetActivityDisposition.Ignored => "ignore",
            PetActivityDisposition.Handled => "handled",
            _ => throw new ArgumentOutOfRangeException(nameof(disposition), disposition, null),
        };

        var response = await _client.PostAsync<PetActivityInboxMutationDto>(
            $"pet-activities/{Uri.EscapeDataString(activity.InboxId)}/{action}",
            new { },
            cancellationToken).ConfigureAwait(false);

        if (!response.Success)
        {
            throw new ChatOSApiException("ChatOS did not accept the pet activity disposition.");
        }
    }
}
