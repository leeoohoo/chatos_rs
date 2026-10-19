import Foundation

public enum MessageTaskInspectorSection: Sendable, Equatable {
    case detail
    case process
    case run
}

/// The durable result of refreshing the selected Inspector section.
///
/// `preserve` is distinct from `clearRun`: the process section keeps an already loaded
/// Run timeline, while a refreshed Task without a latest Run must clear stale Run state.
public enum MessageTaskInspectorSectionUpdate: Sendable, Equatable {
    case preserve
    case clearRun
    case modelOutput(task: MessageTask, runID: String)
    case run(task: MessageTask, detail: MessageTaskRunDetail)
}

/// Application use case that maps an Inspector section to the exact Run query used by macOS.
public struct MessageTaskInspectorRefreshUseCase: Sendable {
    private let runUseCase: MessageTaskRunUseCase

    public init(graphService: any MessageTaskGraphServicing) {
        runUseCase = MessageTaskRunUseCase(graphService: graphService)
    }

    public func refreshSection(
        _ section: MessageTaskInspectorSection,
        messageID: String,
        lookup: MessageTaskLookup?,
        task: MessageTask
    ) async throws -> MessageTaskInspectorSectionUpdate {
        guard let runID = task.lastRunID else { return .clearRun }

        switch section {
        case .process:
            return .preserve
        case .detail:
            let detail = try await runUseCase.loadModelOutput(
                messageID: messageID,
                runID: runID,
                lookup: lookup
            )
            return .modelOutput(
                task: task.merging(run: detail.run),
                runID: runID
            )
        case .run:
            let detail = try await runUseCase.loadLatestRun(
                messageID: messageID,
                runID: runID,
                lookup: lookup
            )
            return .run(
                task: detail.task.merging(run: detail.run),
                detail: detail
            )
        }
    }
}
