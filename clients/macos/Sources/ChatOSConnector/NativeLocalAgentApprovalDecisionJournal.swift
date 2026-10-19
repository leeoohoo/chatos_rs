import ChatOSCore
import Foundation

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
        return try JSONDecoder().decode(
            [NativeLocalAgentApprovalWritebackDecision].self,
            from: data
        ).filter(\.isValid)
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
    private var decisions: [NativeLocalAgentApprovalWritebackDecision]

    init(fileURL: URL? = nil) {
        let store = fileURL.map(NativeLocalAgentApprovalDecisionJournalStore.init(fileURL:))
        self.store = store
        decisions = (try? store?.load()) ?? []
    }

    func decision(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) -> NativeLocalAgentApprovalWritebackDecision? {
        decisions.first {
            $0.matches(ownerUserID: ownerUserID, invocation: invocation)
        }
    }

    func record(_ decision: NativeLocalAgentApprovalWritebackDecision) throws {
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
}
