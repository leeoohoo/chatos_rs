import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation

/// A deliberately small projection of a Run for the task board. Keeping the full Run out of the
/// view tree avoids diffing megabytes of messages/events and parsing tool receipts during layout.
struct TeamTodoRunPresentation: Equatable, Sendable {
    let runID: UUID
    let status: AgentRunCheckpoint.Status
    let receiptCount: Int
    let committedPaths: [String]

    static func presentationsByTodoID(
        runs: [LocalAgentGroupChatRun],
        deliveriesByRunID: [UUID: ProjectAgentDelivery]
    ) -> [String: TeamTodoRunPresentation] {
        var result: [String: TeamTodoRunPresentation] = [:]
        for run in runs {
            guard let delivery = deliveriesByRunID[run.id],
                  delivery.triggerKind == .todo,
                  delivery.deduplicationKey.hasPrefix("todo:") else { continue }
            let todoID = String(delivery.deduplicationKey.dropFirst("todo:".count))
            guard result[todoID] == nil else { continue }
            result[todoID] = .init(run: run)
        }
        return result
    }

    static func presentationsByTodoID(
        summaries: [LocalAgentTodoRunSummary]
    ) -> [String: TeamTodoRunPresentation] {
        Dictionary(uniqueKeysWithValues: summaries.map { summary in
            (summary.todoID, TeamTodoRunPresentation(summary: summary))
        })
    }

    init(summary: LocalAgentTodoRunSummary) {
        runID = summary.runID
        status = summary.status
        receiptCount = summary.receiptCount
        committedPaths = summary.committedPaths
    }

    init(run: LocalAgentGroupChatRun) {
        runID = run.id
        status = run.checkpoint.status
        receiptCount = run.checkpoint.receipts.count

        var paths: Set<String> = []
        for receipt in run.checkpoint.receipts.values where !receipt.isError {
            guard !Task.isCancelled else { break }
            guard let data = receipt.content.data(using: .utf8),
                  let value = try? JSONSerialization.jsonObject(with: data) else { continue }
            Self.collectCommittedPaths(from: value, into: &paths)
        }
        committedPaths = paths.sorted()
    }

    private static func collectCommittedPaths(from value: Any, into paths: inout Set<String>) {
        guard !Task.isCancelled else { return }
        if let object = value as? [String: Any] {
            if let committed = object["committed_paths"] as? [String] {
                paths.formUnion(committed)
            }
            for nested in object.values {
                guard !Task.isCancelled else { return }
                collectCommittedPaths(from: nested, into: &paths)
            }
        } else if let array = value as? [Any] {
            for nested in array {
                guard !Task.isCancelled else { return }
                collectCommittedPaths(from: nested, into: &paths)
            }
        }
    }
}
