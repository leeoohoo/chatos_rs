import ChatOSCore
import Foundation

public struct ChatOSPetActivityInboxService: Sendable {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func fetchOpenActivities(limit: Int = 100) async throws -> [PetActivity] {
        let normalizedLimit = min(max(limit, 1), 500)
        let response: PetActivityInboxListDTO = try await client.request(
            "/pet-activities?include_closed=false&mark_displayed=true&limit=\(normalizedLimit)"
        )
        return response.activities.compactMap(\.model)
    }

    public func apply(
        _ disposition: PetActivityDisposition,
        to activity: PetActivity
    ) async throws {
        guard let inboxID = activity.inboxID?.trimmingCharacters(in: .whitespacesAndNewlines),
              !inboxID.isEmpty else {
            return
        }
        let action: String
        switch disposition {
        case .acknowledged: action = "acknowledge"
        case .ignored: action = "ignore"
        case .handled: action = "handled"
        }
        let encodedID = inboxID.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? inboxID
        let _: PetActivityInboxMutationDTO = try await client.request(
            "/pet-activities/\(encodedID)/\(action)",
            method: "POST",
            body: Data("{}".utf8)
        )
    }
}

private struct PetActivityInboxListDTO: Decodable, Sendable {
    var activities: [PetActivityInboxRecordDTO]

    enum CodingKeys: String, CodingKey { case activities }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        activities = try container.decodeIfPresent(
            [PetActivityInboxRecordDTO].self,
            forKey: .activities
        ) ?? []
    }
}

private struct PetActivityInboxMutationDTO: Decodable, Sendable {
    var success: Bool
}

struct PetActivityInboxRecordDTO: Decodable, Sendable {
    var id: String
    var activityKey: String
    var activityVersion: String
    var source: String
    var kind: String
    var title: String
    var detail: String?
    var route: PetActivityRouteDTO
    var inboxStatus: String
    var eventID: String?
    var eventSequence: Int64?
    var occurredAt: String
    var updatedAt: String
    var expiresAt: String?

    enum CodingKeys: String, CodingKey {
        case id, source, kind, title, detail, route
        case activityKey = "activity_key"
        case activityVersion = "activity_version"
        case inboxStatus = "inbox_status"
        case eventID = "event_id"
        case eventSequence = "event_sequence"
        case occurredAt = "occurred_at"
        case updatedAt = "updated_at"
        case expiresAt = "expires_at"
    }

    var model: PetActivity? {
        guard let source = mappedSource,
              let kind = mappedKind,
              let inboxStatus = PetActivityInboxStatus(rawValue: inboxStatus) else {
            return nil
        }
        return PetActivity(
            id: activityKey,
            source: source,
            kind: kind,
            title: title,
            detail: detail,
            route: route.model,
            eventID: eventID,
            eventSequence: eventSequence,
            inboxID: id,
            inboxStatus: inboxStatus,
            activityVersion: activityVersion,
            updatedAt: Self.date(updatedAt) ?? Self.date(occurredAt) ?? Date(),
            expiresAt: expiresAt.flatMap(Self.date)
        )
    }

    private var mappedSource: PetActivitySource? {
        switch source {
        case "local_approval": .localApproval
        case "ask_user_prompt": .askUserPrompt
        case "chat": .chat
        case "task_board": .taskBoard
        case "task_runner": .taskRunner
        case "project_execution": .projectExecution
        default: nil
        }
    }

    private var mappedKind: PetActivityKind? {
        switch kind {
        case "working": .working
        case "reviewing": .reviewing
        case "waiting_for_approval": .waitingForApproval
        case "waiting_for_user": .waitingForUser
        case "succeeded": .succeeded
        case "failed": .failed
        case "blocked": .blocked
        case "cancelled", "canceled": .cancelled
        default: nil
        }
    }

    private static func date(_ value: String) -> Date? {
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractional.date(from: value) ?? ISO8601DateFormatter().date(from: value)
    }
}

struct PetActivityRouteDTO: Decodable, Sendable {
    var projectID: String?
    var conversationID: String?
    var turnID: String?
    var messageID: String?
    var promptID: String?
    var taskID: String?
    var runID: String?

    enum CodingKeys: String, CodingKey {
        case projectID = "project_id"
        case conversationID = "conversation_id"
        case turnID = "turn_id"
        case messageID = "message_id"
        case promptID = "prompt_id"
        case taskID = "task_id"
        case runID = "run_id"
    }

    var model: PetActivityRoute {
        PetActivityRoute(
            projectID: projectID,
            conversationID: conversationID,
            turnID: turnID,
            messageID: messageID,
            promptID: promptID,
            taskID: taskID,
            runID: runID
        )
    }
}
