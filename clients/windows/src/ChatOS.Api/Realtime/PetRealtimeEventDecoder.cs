using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Api.Pet;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Realtime;

public static class PetRealtimeEventDecoder
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    public static PetActivityEvent? Decode(string json)
    {
        var envelope = JsonSerializer.Deserialize<PetRealtimeEnvelopeDto>(json, JsonOptions);
        if (envelope is null ||
            !string.Equals(envelope.Type, "event", StringComparison.Ordinal) ||
            envelope.Payload is null ||
            !string.Equals(envelope.Payload.Kind, "pet_activity_inbox_updated", StringComparison.Ordinal))
        {
            return null;
        }

        var activity = envelope.Payload.Activity?.ToDomain();
        if (activity is null)
        {
            return new PetActivityEvent.Reconcile();
        }

        activity = activity with
        {
            EventId = envelope.EventId,
            EventSequence = envelope.EventSequence,
        };

        return activity.InboxStatus switch
        {
            PetActivityInboxStatus.Unread or PetActivityInboxStatus.Displayed =>
                new PetActivityEvent.Upsert(activity),
            PetActivityInboxStatus.Acknowledged or
            PetActivityInboxStatus.Ignored or
            PetActivityInboxStatus.Handled or
            PetActivityInboxStatus.Resolved or
            PetActivityInboxStatus.Expired => new PetActivityEvent.Remove(activity.Id),
            _ => new PetActivityEvent.Reconcile(),
        };
    }
}

internal sealed record PetRealtimeEnvelopeDto
{
    [JsonPropertyName("type")]
    public required string Type { get; init; }

    [JsonPropertyName("event_id")]
    public string? EventId { get; init; }

    [JsonPropertyName("event_sequence")]
    public long? EventSequence { get; init; }

    [JsonPropertyName("payload")]
    public PetRealtimePayloadDto? Payload { get; init; }
}

internal sealed record PetRealtimePayloadDto
{
    [JsonPropertyName("kind")]
    public required string Kind { get; init; }

    [JsonPropertyName("activity")]
    public PetActivityInboxRecordDto? Activity { get; init; }
}
