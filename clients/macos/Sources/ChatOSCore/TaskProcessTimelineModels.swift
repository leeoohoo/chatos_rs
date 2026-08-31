import Foundation

public struct TaskProcessTimelineItem: Identifiable, Sendable, Equatable {
    public let id: String
    public var title: String
    public var detail: String
    public var occurredAt: String?
    public var status: String

    public init(
        id: String,
        title: String,
        detail: String,
        occurredAt: String? = nil,
        status: String
    ) {
        self.id = id
        self.title = title
        self.detail = detail
        self.occurredAt = occurredAt
        self.status = status
    }
}

public enum TaskProcessTimelineBuilder {
    public static func build(
        processLog: String?,
        taskStatus: String?
    ) -> [TaskProcessTimelineItem] {
        let entries = parse(processLog)
        let finalStatus = normalizedStatus(taskStatus) ?? "succeeded"

        return entries.enumerated().map { index, entry in
            TaskProcessTimelineItem(
                id: "task-process-\(index)-\(entry.occurredAt ?? "record")",
                title: entry.title,
                detail: entry.detail,
                occurredAt: entry.occurredAt,
                status: index == entries.count - 1 ? finalStatus : "succeeded"
            )
        }
    }

    private struct ParsedEntry {
        var title: String
        var detail: String
        var occurredAt: String?
    }

    private static func parse(_ processLog: String?) -> [ParsedEntry] {
        guard let processLog = processLog?.trimmingCharacters(in: .whitespacesAndNewlines),
              !processLog.isEmpty else {
            return []
        }

        var entries: [ParsedEntry] = []
        var current: ParsedEntry?

        func appendCurrent() {
            guard var entry = current else { return }
            entry.detail = entry.detail.trimmingCharacters(in: .whitespacesAndNewlines)
            if entry.detail.isEmpty { entry.detail = "暂无过程说明" }
            entries.append(entry)
            current = nil
        }

        for line in processLog.replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\r", with: "\n")
            .split(separator: "\n", omittingEmptySubsequences: false)
            .map(String.init) {
            if let header = parseHeader(line) {
                appendCurrent()
                current = ParsedEntry(
                    title: header.title,
                    detail: "",
                    occurredAt: header.occurredAt
                )
                continue
            }

            guard !line.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || current != nil else {
                continue
            }
            if current == nil {
                current = ParsedEntry(title: "过程记录", detail: line, occurredAt: nil)
            } else if var entry = current {
                entry.detail += entry.detail.isEmpty ? line : "\n\(line)"
                current = entry
            }
        }

        appendCurrent()
        return entries
    }

    private static func parseHeader(_ line: String) -> (occurredAt: String, title: String)? {
        guard line.first == "[", let closing = line.firstIndex(of: "]") else { return nil }
        let timestamp = line[line.index(after: line.startIndex)..<closing]
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !timestamp.isEmpty else { return nil }
        let title = line[line.index(after: closing)...]
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return (timestamp, title.isEmpty ? "过程记录" : title)
    }

    private static func normalizedStatus(_ status: String?) -> String? {
        let value = status?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return value?.isEmpty == false ? value : nil
    }
}
