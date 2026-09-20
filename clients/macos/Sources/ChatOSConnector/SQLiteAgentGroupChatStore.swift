import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

public struct ProjectAgentArtifactUploadJob: Sendable, Equatable {
    public let attachmentID: String
    public let roomID: String
    public let attempt: Int
    public let request: AgentArtifactUploadRequest

    public init(
        attachmentID: String,
        roomID: String,
        attempt: Int,
        request: AgentArtifactUploadRequest
    ) {
        self.attachmentID = attachmentID
        self.roomID = roomID
        self.attempt = attempt
        self.request = request
    }
}

/// Account- and project-scoped local authority for Agent rooms. The transcript and delivery
/// queue remain usable without the network, Memory Engine, Plugin Management or Codex CLI.
public actor SQLiteAgentGroupChatStore: AgentGroupChatStore, LocalAgentGroupChatRunStoring {
    nonisolated(unsafe) var database: OpaquePointer?
    let attachmentsRootURL: URL
    let agentArtifactService: (any AgentArtifactRemoteServing)?
#if DEBUG
    var debugPreparedStatementCount = 0
#endif

    public init(
        databaseURL: URL,
        agentArtifactService: (any AgentArtifactRemoteServing)? = nil
    ) throws {
        attachmentsRootURL = databaseURL.deletingLastPathComponent()
            .appendingPathComponent("AgentGroupChatAttachments", isDirectory: true)
        self.agentArtifactService = agentArtifactService
        database = try AgentGroupChatDatabase.open(at: databaseURL)
    }

    deinit { AgentGroupChatDatabase.close(database) }

#if DEBUG
    /// Test-only counter for repeatable Store baselines. Release builds do not carry the counter.
    func preparedStatementCountForTesting() -> Int {
        debugPreparedStatementCount
    }

    func totalDatabaseChangesForTesting() -> Int64 {
        Int64(sqlite3_total_changes(database))
    }
#endif

    /// Creates a Run-scoped staging directory beneath the existing protected attachment root.
    /// The opaque directory name is never exposed to the model and is removed with the Run vault.
    public func createAgentDocumentDraftDirectory() throws -> URL {
        let draftsRoot = attachmentsRootURL.appendingPathComponent(".drafts", isDirectory: true)
        try FileManager.default.createDirectory(
            at: draftsRoot,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        let directory = draftsRoot.appendingPathComponent(
            UUID().uuidString.lowercased(),
            isDirectory: true
        )
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700]
        )
        return directory
    }

}
