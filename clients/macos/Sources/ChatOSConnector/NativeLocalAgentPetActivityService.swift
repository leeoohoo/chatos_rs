import ChatOSCore
import Foundation

public actor NativeLocalAgentPetActivityService: PetActivityStreaming {
    private let client: NativeLocalAgentRuntimeClient
    private var ownerUserID: String?

    public init(host: any LocalAgentHostClientServicing) {
        self.client = NativeLocalAgentRuntimeClient(host: host)
    }

    public func configure(ownerUserID: String) {
        self.ownerUserID = ownerUserID
    }

    public func reset() {
        ownerUserID = nil
    }

    public func fetchOpenActivities(limit: Int = 100) async throws -> [PetActivity] {
        guard let ownerUserID else {
            throw NativeLocalAgentPetActivityServiceError.notConfigured
        }
        async let active = client.listRuns(
            ownerUserID: ownerUserID,
            scope: "active",
            limit: UInt32(max(1, min(100, limit)))
        )
        async let terminal = client.listRuns(
            ownerUserID: ownerUserID,
            scope: "terminal",
            limit: UInt32(max(1, min(100, limit)))
        )
        let pages = try await (active, terminal)
        let recentCutoff = Date().addingTimeInterval(-15 * 60)
        return (pages.0.runs + pages.1.runs)
            .filter { run in
                !Self.isTerminal(run.status)
                    || Self.date(run.updatedAtUnixMs) >= recentCutoff
            }
            .prefix(max(1, limit))
            .map(Self.activity)
    }

    public func apply(
        _ disposition: PetActivityDisposition,
        to activity: PetActivity
    ) async throws {
        // Local activities are projections of durable Run state. Dismissal is
        // intentionally UI-local; it must not mutate or delete execution facts.
    }

    public func petActivityEvents() async -> AsyncThrowingStream<PetActivityEvent, Error> {
        AsyncThrowingStream { continuation in
            let task = Task { [weak self] in
                var currentOwner: String?
                var cursor: Int64 = 0
                var idleDelay = NativeLocalAgentEventPollingPolicy.activeDelay
                while !Task.isCancelled {
                    guard let self else { return }
                    guard let owner = await self.configuredOwner() else {
                        try? await Task.sleep(for: .milliseconds(400))
                        continue
                    }
                    if owner != currentOwner {
                        currentOwner = owner
                        cursor = 0
                        idleDelay = NativeLocalAgentEventPollingPolicy.activeDelay
                        continuation.yield(.reconcile)
                    }
                    do {
                        let page = try await self.client.listEvents(
                            ownerUserID: owner,
                            afterCursor: cursor
                        )
                        guard await self.configuredOwner() == owner else { continue }
                        cursor = page.nextCursor
                        if !page.events.isEmpty {
                            idleDelay = NativeLocalAgentEventPollingPolicy.activeDelay
                            continuation.yield(.reconcile)
                        } else {
                            try await Task.sleep(for: idleDelay)
                            idleDelay = NativeLocalAgentEventPollingPolicy.nextIdleDelay(
                                after: idleDelay
                            )
                        }
                    } catch is CancellationError {
                        return
                    } catch {
                        try? await Task.sleep(for: .seconds(1))
                    }
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    private func configuredOwner() -> String? {
        ownerUserID
    }

    private static func activity(_ run: LocalAgentRunRecord) -> PetActivity {
        let source: PetActivitySource = run.profileKey == "main_chat" ? .chat : .taskExecution
        let conversationID = string("conversation_id", in: run.input)
            ?? string("source_conversation_id", in: run.input)
        let turnID = string("turn_id", in: run.input)
            ?? string("source_turn_id", in: run.input)
            ?? (run.ownerEntityType == "conversation_turn" ? run.ownerEntityID : nil)
        let taskID = run.ownerEntityType == "task" ? run.ownerEntityID : nil
        let updatedAt = date(run.updatedAtUnixMs)
        let kind = activityKind(run.status)
        return PetActivity(
            id: "local-run:\(run.runID)",
            source: source,
            kind: kind,
            title: source == .chat ? "本地对话" : "本地任务",
            detail: detail(run),
            route: .init(
                conversationID: conversationID,
                turnID: turnID,
                promptID: run.status == "waiting_user" ? "local-ask:\(run.runID)" : nil,
                taskID: taskID,
                runID: run.runID
            ),
            eventID: "local-run:\(run.runID):\(run.version)",
            eventSequence: Int64(clamping: run.version),
            activityVersion: String(run.version),
            updatedAt: updatedAt,
            expiresAt: kind == .succeeded || kind == .cancelled
                ? updatedAt.addingTimeInterval(15)
                : nil
        )
    }

    private static func activityKind(_ status: String) -> PetActivityKind {
        switch status {
        case "waiting_user": .waitingForUser
        case "paused", "needs_review": .reviewing
        case "succeeded": .succeeded
        case "failed": .failed
        case "cancelled": .cancelled
        default: .working
        }
    }

    private static func detail(_ run: LocalAgentRunRecord) -> String? {
        if let outcome = run.terminalOutcome {
            return string("error", in: outcome)
                ?? string("text", in: outcome)
                ?? string("reason", in: outcome)
        }
        return switch run.status {
        case "waiting_user": "等待你的回复"
        case "waiting_tool_result": "正在执行本地工具"
        case "retry_scheduled": "等待本地重试"
        case "needs_review": "需要检查执行结果"
        default: nil
        }
    }

    private static func string(_ key: String, in value: LocalAgentJSONValue) -> String? {
        guard case let .object(object) = value,
              case let .string(result)? = object[key] else { return nil }
        return result
    }

    private static func isTerminal(_ status: String) -> Bool {
        status == "succeeded" || status == "failed" || status == "cancelled"
    }

    private static func date(_ unixMilliseconds: Int64) -> Date {
        Date(timeIntervalSince1970: Double(unixMilliseconds) / 1_000)
    }
}

public enum NativeLocalAgentPetActivityServiceError: LocalizedError {
    case notConfigured

    public var errorDescription: String? {
        "Local Agent pet activity service is not configured."
    }
}
