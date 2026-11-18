import ChatOSCore
import Foundation

enum NativeLocalAgentApprovalDecisionJournalError: LocalizedError {
    case unavailable(path: String, reason: String)

    var errorDescription: String? {
        switch self {
        case let .unavailable(path, reason):
            "本机审批决定日志损坏或不可读，已停止自动审批恢复：\(path)（\(reason)）"
        }
    }
}

struct NativeLocalAgentApprovalWritebackDecision: Codable, Sendable, Equatable {
    let ownerUserID: String
    let invocationID: String
    let invocationVersion: UInt64
    let approve: Bool
    let decidedBy: String
    let reason: String
    let recordedAtUnixMs: Int64

    func matches(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) -> Bool {
        self.ownerUserID == ownerUserID
            && invocationID == invocation.invocationID
            && invocationVersion == invocation.version
    }

    fileprivate var isValid: Bool {
        !ownerUserID.isEmpty && ownerUserID.utf8.count <= 1_024
            && !invocationID.isEmpty && invocationID.utf8.count <= 1_024
            && invocationVersion > 0
            && !decidedBy.isEmpty && decidedBy.utf8.count <= 1_024
            && reason.count <= 4_000
            && recordedAtUnixMs > 0
    }
}

private struct NativeLocalAgentApprovalDecisionJournalStore: Sendable {
    static let maximumBytes = 256 * 1_024
    static let maximumEntries = 256

    let fileURL: URL

    func load() throws -> [NativeLocalAgentApprovalWritebackDecision] {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return [] }
        let data = try NativeBoundedFileReader.read(
            fileURL,
            maximumBytes: Self.maximumBytes
        )
        let decisions = try JSONDecoder().decode(
            [NativeLocalAgentApprovalWritebackDecision].self,
            from: data
        )
        guard decisions.allSatisfy(\.isValid) else {
            throw DecodingError.dataCorrupted(.init(
                codingPath: [],
                debugDescription: "approval decision journal contains invalid entries"
            ))
        }
        return decisions
    }

    func save(_ decisions: [NativeLocalAgentApprovalWritebackDecision]) throws {
        try FileManager.default.createDirectory(
            at: fileURL.deletingLastPathComponent(),
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        let data = try encoder.encode(decisions)
        guard data.count <= Self.maximumBytes else {
            throw NativeBoundedFileReadError.fileTooLarge(
                maximumBytes: Self.maximumBytes
            )
        }
        try data.write(to: fileURL, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: fileURL.path
        )
    }
}

actor NativeLocalAgentApprovalDecisionJournal {
    private let store: NativeLocalAgentApprovalDecisionJournalStore?
    private let loadError: NativeLocalAgentApprovalDecisionJournalError?
    private var decisions: [NativeLocalAgentApprovalWritebackDecision]

    init(fileURL: URL? = nil) {
        let store = fileURL.map(NativeLocalAgentApprovalDecisionJournalStore.init(fileURL:))
        self.store = store
        do {
            decisions = try store?.load() ?? []
            loadError = nil
        } catch {
            decisions = []
            loadError = .unavailable(
                path: fileURL?.path ?? "",
                reason: error.localizedDescription
            )
        }
    }

    func decision(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) throws -> NativeLocalAgentApprovalWritebackDecision? {
        try requireAvailable()
        return decisions.first {
            $0.matches(ownerUserID: ownerUserID, invocation: invocation)
        }
    }

    func record(_ decision: NativeLocalAgentApprovalWritebackDecision) throws {
        try requireAvailable()
        guard decision.isValid else { return }
        decisions.removeAll {
            $0.ownerUserID == decision.ownerUserID
                && $0.invocationID == decision.invocationID
                && $0.invocationVersion == decision.invocationVersion
        }
        decisions.insert(decision, at: 0)
        decisions = Array(decisions.prefix(
            NativeLocalAgentApprovalDecisionJournalStore.maximumEntries
        ))
        try store?.save(decisions)
    }

    func remove(_ decision: NativeLocalAgentApprovalWritebackDecision) throws {
        try requireAvailable()
        decisions.removeAll {
            $0.ownerUserID == decision.ownerUserID
                && $0.invocationID == decision.invocationID
                && $0.invocationVersion == decision.invocationVersion
        }
        try store?.save(decisions)
    }

    func snapshot() -> [NativeLocalAgentApprovalWritebackDecision] {
        decisions
    }

    private func requireAvailable() throws {
        if let loadError { throw loadError }
    }
}
