import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
import Foundation

public struct NativeLocalTemporaryFileMaintenanceReport: Sendable, Equatable {
    public let visitedEntries: Int
    public let eligibleEntries: Int
    public let deletedEntries: Int
    public let lockedEntries: Int
    public let truncated: Bool
}

public struct NativeLocalTemporaryFileMaintenanceService: Sendable {
    private let rootURL: URL
    private let codeReadWorker: any CodeReadWorkerExecuting

    public init(
        rootURL: URL,
        codeReadWorker: any CodeReadWorkerExecuting = CodeReadWorkerProcessClient.shared
    ) {
        self.rootURL = rootURL.standardizedFileURL
        self.codeReadWorker = codeReadWorker
    }

    public func cleanupStaleFiles(
        minimumAgeSeconds: Int = 24 * 60 * 60
    ) async throws -> NativeLocalTemporaryFileMaintenanceReport {
        guard rootURL.isFileURL, minimumAgeSeconds >= 24 * 60 * 60 else {
            throw NativeLocalTemporaryFileMaintenanceError.invalidConfiguration
        }
        let result = try await codeReadWorker.execute(
            operation: "cleanup_stale_temporary_files_v1",
            arguments: [
                "minimum_age_seconds": .number(Double(minimumAgeSeconds)),
                "scopes": .array([
                    .object([
                        "path": .string("LocalAgent/Attachments"),
                        "maximum_depth": .number(2),
                    ]),
                    .object([
                        "path": .string("LocalAgent/local-agent.artifact-staging"),
                        "maximum_depth": .number(0),
                    ]),
                    .object([
                        "path": .string("AgentGroupChatAttachments"),
                        "maximum_depth": .number(2),
                    ]),
                ]),
            ],
            projectRoot: rootURL,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepURL: nil
        )
        guard case let .object(fields) = result,
              let visited = integer(fields["visited_entries"]),
              let eligible = integer(fields["eligible_entries"]),
              let deleted = integer(fields["deleted_entries"]),
              let locked = integer(fields["locked_entries"]),
              case let .bool(truncated)? = fields["truncated"],
              visited >= 0, eligible >= 0, deleted >= 0, locked >= 0,
              deleted <= eligible, locked <= eligible else {
            throw NativeLocalTemporaryFileMaintenanceError.invalidResponse
        }
        return .init(
            visitedEntries: visited,
            eligibleEntries: eligible,
            deletedEntries: deleted,
            lockedEntries: locked,
            truncated: truncated
        )
    }

    private func integer(_ value: CodeReadWorkerJSONValue?) -> Int? {
        guard case let .number(raw)? = value, raw.isFinite, raw.rounded() == raw,
              raw >= 0, raw <= Double(Int.max) else { return nil }
        return Int(raw)
    }
}

public enum NativeLocalTemporaryFileMaintenanceError: Error {
    case invalidConfiguration
    case invalidResponse
}
