import ChatOSCore
import Foundation

struct PetRealtimeEnvelopeDTO: Decodable, Sendable {
    var type: String
    var event: String
    var eventID: String
    var eventSequence: Int64
    var conversationID: String?
    var projectID: String?
    var payload: PetRealtimePayloadDTO?
    var timestamp: String

    enum CodingKeys: String, CodingKey {
        case type, event, payload
        case eventID = "event_id"
        case eventSequence = "event_sequence"
        case conversationID = "conversation_id"
        case projectID = "project_id"
        case timestamp = "ts"
    }
}

struct PetRealtimePayloadDTO: Decodable, Sendable {
    var kind: String
    var activity: PetActivityInboxRecordDTO?

    enum CodingKeys: String, CodingKey {
        case kind, activity
    }
}

extension PetRealtimeEnvelopeDTO {
    func petActivityEvent() -> PetActivityEvent? {
        guard type == "event", let payload else { return nil }
        switch payload.kind {
        case "pet_activity_inbox_updated":
            return inboxActivityEvent(payload)
        default:
            return nil
        }
    }

    private func inboxActivityEvent(_ payload: PetRealtimePayloadDTO) -> PetActivityEvent? {
        guard var activity = payload.activity?.model else { return .reconcile }
        activity.eventID = eventID
        activity.eventSequence = eventSequence
        switch activity.inboxStatus {
        case .unread, .displayed:
            return .upsert(activity)
        case .acknowledged, .ignored, .handled, .resolved, .expired:
            return .remove(id: activity.id)
        case nil:
            return .reconcile
        }
    }
}
