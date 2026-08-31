import ChatOSCore
import Foundation

struct MessageTaskRunSummaryDTO: Decodable, Sendable {
    var id: String
    var status: String?
    var modelPhaseStatus: String?
    var resultSummary: String?
    var report: JSONValue?
    var errorMessage: String?
    var startedAt: String?
    var finishedAt: String?

    enum CodingKeys: String, CodingKey {
        case id, status, report
        case modelPhaseStatus = "model_phase_status"
        case resultSummary = "result_summary"
        case errorMessage = "error_message"
        case startedAt = "started_at"
        case finishedAt = "finished_at"
    }
}

struct MessageTaskRunDetailDTO: Decodable, Sendable {
    var task: MessageTaskDTO
    var run: MessageTaskRunDTO
    var events: [MessageTaskRunEventDTO]
    var eventsTotal: Int?
    var eventsHasMore: Bool?

    enum CodingKeys: String, CodingKey {
        case task, run, events
        case eventsTotal = "events_total"
        case eventsHasMore = "events_has_more"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        task = try container.decode(MessageTaskDTO.self, forKey: .task)
        run = try container.decode(MessageTaskRunDTO.self, forKey: .run)
        events = try container.decodeIfPresent([MessageTaskRunEventDTO].self, forKey: .events) ?? []
        eventsTotal = try container.decodeIfPresent(Int.self, forKey: .eventsTotal)
        eventsHasMore = try container.decodeIfPresent(Bool.self, forKey: .eventsHasMore)
    }
}

struct MessageTaskRunDTO: Decodable, Sendable {
    var id: String
    var taskID: String
    var status: String?
    var modelPhaseStatus: String?
    var startedAt: String?
    var finishedAt: String?
    var resultSummary: String?
    var report: JSONValue?
    var errorMessage: String?

    enum CodingKeys: String, CodingKey {
        case id, status, report
        case taskID = "task_id"
        case modelPhaseStatus = "model_phase_status"
        case startedAt = "started_at"
        case finishedAt = "finished_at"
        case resultSummary = "result_summary"
        case errorMessage = "error_message"
    }
}

struct MessageTaskRunEventDTO: Decodable, Sendable {
    var id: String
    var eventType: String
    var message: String?
    var createdAt: String?

    enum CodingKeys: String, CodingKey {
        case id, message
        case eventType = "event_type"
        case createdAt = "created_at"
    }
}

struct MessageTaskRetryResponseDTO: Decodable, Sendable {
    var success: Bool
    var run: MessageTaskRunDTO
}

extension MessageTaskRunDetailDTO {
    var model: MessageTaskRunDetail {
        MessageTaskRunDetail(
            task: task.model,
            run: run.model,
            events: events.map {
                MessageTaskRunEvent(
                    id: $0.id,
                    eventType: $0.eventType,
                    message: $0.message?.trimmedNonEmptyValue,
                    createdAt: DateParser.parse($0.createdAt)
                )
            },
            eventsTotal: eventsTotal ?? events.count,
            eventsHasMore: eventsHasMore ?? false
        )
    }
}

extension MessageTaskRunDTO {
    var model: MessageTaskRun {
        MessageTaskRun(
            id: id,
            taskID: taskID,
            status: status,
            modelPhaseStatus: modelPhaseStatus,
            startedAt: DateParser.parse(startedAt),
            finishedAt: DateParser.parse(finishedAt),
            resultSummary: resultSummary?.trimmedNonEmptyValue,
            reportContent: report?.reportContent,
            errorMessage: errorMessage?.trimmedNonEmptyValue
        )
    }
}

extension MessageTaskRunSummaryDTO {
    var modelSummary: MessageTaskLastRunSummary {
        MessageTaskLastRunSummary(
            id: id,
            status: status?.trimmedNonEmptyValue,
            modelPhaseStatus: modelPhaseStatus?.trimmedNonEmptyValue,
            resultSummary: resultSummary?.trimmedNonEmptyValue,
            reportContent: report?.reportContent,
            errorMessage: errorMessage?.trimmedNonEmptyValue,
            startedAt: DateParser.parse(startedAt),
            finishedAt: DateParser.parse(finishedAt)
        )
    }
}
