import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
import ChatOSCore
import ChatOSMediaWorkerProtocol
import CryptoKit
import Darwin
import Foundation

struct NativeLocalAgentResolvedAttachment: Sendable, Equatable {
    let displayName: String
    let mediaType: String
    let byteSize: UInt64
    let sha256: String
    let offset: UInt64
    let nextOffset: UInt64?
    let encoding: String
    let content: String

    var jsonValue: LocalAgentJSONValue {
        .object([
            "display_name": .string(displayName), "media_type": .string(mediaType),
            "byte_size": .number(Double(byteSize)), "sha256": .string(sha256),
            "offset": .number(Double(offset)),
            "next_offset": nextOffset.map { .number(Double($0)) } ?? .null,
            "encoding": .string(encoding), "content": .string(content),
        ])
    }
}

struct NativeLocalAgentVerifiedAttachmentFile: Sendable, Equatable {
    let url: URL
    let identity: MediaWorkerFileIdentity
}

struct NativeLocalAgentAttachmentVault: Sendable {
    static let referencePrefix = "local-attachment:"
    static let maximumAttachmentBytes = 20 * 1_024 * 1_024
    static let maximumReadBytes = 64 * 1_024
    static let maximumInlineReadBytes = 2 * 1_024

    let rootURL: URL
    private let codeReadWorker: any CodeReadWorkerExecuting
    private let binaryCodeReadWorker: any CodeReadWorkerBinaryExecuting

    init(
        rootURL: URL,
        codeReadWorker: any CodeReadWorkerExecuting = CodeReadWorkerProcessClient.shared,
        binaryCodeReadWorker: any CodeReadWorkerBinaryExecuting = CodeReadWorkerProcessClient.shared
    ) {
        self.rootURL = rootURL
        self.codeReadWorker = codeReadWorker
        self.binaryCodeReadWorker = binaryCodeReadWorker
    }

    /// Production send path. The already-bounded draft buffers are written directly to a
    /// one-request Worker pipe; they are never duplicated into JSON/Base64 in the App process.
    func authorizeInWorker(
        _ drafts: [ConversationAttachmentDraft],
        ownerUserID: String,
        conversationID: String
    ) async throws -> [LocalAgentConversationAttachmentSpec] {
        guard !drafts.isEmpty else { return [] }
        var totalBytes = 0
        var items: [CodeReadWorkerJSONValue] = []
        var tokens: [String] = []
        items.reserveCapacity(drafts.count)
        tokens.reserveCapacity(drafts.count)
        for draft in drafts {
            let (nextTotal, overflow) = totalBytes.addingReportingOverflow(draft.data.count)
            guard !draft.data.isEmpty,
                  draft.data.count <= Self.maximumAttachmentBytes,
                  !overflow,
                  nextTotal <= ChatOSCodeReadWorkerProtocol.maximumBinaryPayloadBytes else {
                throw NativeLocalAgentAttachmentVaultError.invalidAttachment
            }
            totalBytes = nextTotal
            let token = UUID().uuidString.lowercased()
            tokens.append(token)
            items.append(.object([
                "token": .string(token),
                "byte_size": .number(Double(draft.data.count)),
            ]))
        }
        try FileManager.default.createDirectory(
            at: rootURL,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        let result: CodeReadWorkerJSONValue
        do {
            result = try await binaryCodeReadWorker.execute(
                operation: "attachment_authorize_v1",
                arguments: [
                    "owner_user_id": .string(ownerUserID),
                    "conversation_id": .string(conversationID),
                    "items": .array(items),
                ],
                binaryPayloads: drafts.map(\.data),
                projectRoot: rootURL,
                requestCWD: nil,
                defaultToolRoot: nil,
                ripgrepURL: nil
            )
        } catch let error as CodeReadWorkerProcessClientError {
            guard case let .requestRejected(code, _) = error else { throw error }
            switch code {
            case "attachment_reference_invalid":
                throw NativeLocalAgentAttachmentVaultError.invalidReference
            case "attachment_integrity_mismatch":
                throw NativeLocalAgentAttachmentVaultError.integrityMismatch
            default:
                throw NativeLocalAgentAttachmentVaultError.invalidAttachment
            }
        }
        guard case let .object(object) = result,
              case let .array(rawResults)? = object["items"],
              rawResults.count == drafts.count else {
            throw NativeLocalAgentAttachmentWorkerShapeError.invalid
        }
        var specs: [LocalAgentConversationAttachmentSpec] = []
        specs.reserveCapacity(drafts.count)
        for index in drafts.indices {
            let draft = drafts[index]
            let token = tokens[index]
            guard case let .object(fields) = rawResults[index],
                  case let .string(returnedToken)? = fields["token"],
                  returnedToken == token,
                  case let .number(returnedSize)? = fields["byte_size"],
                  returnedSize == Double(draft.data.count),
                  case let .string(digest)? = fields["sha256"],
                  digest.count == 64,
                  digest.utf8.allSatisfy({
                      (48...57).contains($0) || (97...102).contains($0)
                  }) else {
                throw NativeLocalAgentAttachmentWorkerShapeError.invalid
            }
            specs.append(.init(
                attachmentID: draft.id,
                displayName: draft.name,
                mediaType: draft.mimeType,
                byteSize: UInt64(draft.data.count),
                sha256: digest,
                authorizedLocalRef: Self.referencePrefix + token,
                metadata: .object(["kind": .string(draft.kind.rawValue)])
            ))
        }
        return specs
    }

    func resolve(
        _ record: LocalAgentConversationAttachmentRecord,
        ownerUserID: String,
        conversationID: String,
        offset: UInt64,
        limit: Int
    ) async throws -> NativeLocalAgentResolvedAttachment {
        guard (1...Self.maximumReadBytes).contains(limit), offset <= record.byteSize else {
            throw NativeLocalAgentAttachmentVaultError.invalidAttachment
        }
        var workerArguments = try arguments(
            record,
            ownerUserID: ownerUserID,
            conversationID: conversationID
        )
        workerArguments["offset"] = .number(Double(offset))
        workerArguments["limit"] = .number(Double(limit))
        let object = try await execute(
            operation: "attachment_resolve_v1",
            arguments: workerArguments
        ).attachmentObject()
        return .init(
            displayName: try object.attachmentString("display_name"),
            mediaType: try object.attachmentString("media_type"),
            byteSize: try object.attachmentUInt64("byte_size"),
            sha256: try object.attachmentString("sha256"),
            offset: try object.attachmentUInt64("offset"),
            nextOffset: object.attachmentOptionalUInt64("next_offset"),
            encoding: try object.attachmentString("encoding"),
            content: try object.attachmentString("content")
        )
    }

    func verifiedFile(
        _ record: LocalAgentConversationAttachmentRecord,
        ownerUserID: String,
        conversationID: String
    ) async throws -> NativeLocalAgentVerifiedAttachmentFile {
        let object = try await execute(
            operation: "attachment_verify_file_v1",
            arguments: try arguments(
                record,
                ownerUserID: ownerUserID,
                conversationID: conversationID
            )
        ).attachmentObject()
        let relativePath = try object.attachmentString("relative_path")
        return .init(
            url: rootURL.appendingPathComponent(relativePath, isDirectory: false),
            identity: .init(
                device: try object.attachmentUInt64String("device"),
                inode: try object.attachmentUInt64String("inode"),
                byteCount: Int64(try object.attachmentUInt64("byte_count"))
            )
        )
    }

    func materialize(
        _ record: LocalAgentConversationAttachmentRecord,
        ownerUserID: String,
        conversationID: String,
        destinationRootURL: URL
    ) async throws -> NativeLocalAgentWorkspaceAttachment {
        var status = stat()
        guard destinationRootURL.path.withCString({ Darwin.lstat($0, &status) }) == 0,
              (status.st_mode & S_IFMT) == S_IFDIR else {
            throw NativeLocalAgentAttachmentWorkspaceError.unavailable
        }
        var workerArguments = try arguments(
            record,
            ownerUserID: ownerUserID,
            conversationID: conversationID
        )
        workerArguments["destination_root_path"] = .string(destinationRootURL.path)
        workerArguments["destination_root_device"] = .string(String(status.st_dev))
        workerArguments["destination_root_inode"] = .string(String(status.st_ino))
        let object = try await execute(
            operation: "attachment_materialize_v1",
            arguments: workerArguments
        ).attachmentObject()
        return .init(
            relativePath: try object.attachmentString("relative_path"),
            record: record
        )
    }

    func previewURL(
        _ record: LocalAgentConversationAttachmentRecord,
        ownerUserID: String,
        conversationID: String
    ) throws -> URL {
        _ = try arguments(record, ownerUserID: ownerUserID, conversationID: conversationID)
        let token = String(record.authorizedLocalRef.dropFirst(Self.referencePrefix.count))
        return scopedDirectory(ownerUserID: ownerUserID, conversationID: conversationID)
            .appendingPathComponent(token, isDirectory: false)
    }

    private func execute(
        operation: String,
        arguments: [String: NativeJSONValue]
    ) async throws -> NativeJSONValue {
        do {
            return try await NativeCodeReadWorkerBridge.execute(
                name: operation,
                arguments: arguments,
                projectRoot: rootURL,
                requestCWD: nil,
                defaultToolRoot: nil,
                client: codeReadWorker
            )
        } catch let error as CodeReadWorkerProcessClientError {
            guard case let .requestRejected(code, _) = error else { throw error }
            switch code {
            case "attachment_reference_invalid":
                throw NativeLocalAgentAttachmentVaultError.invalidReference
            case "attachment_integrity_mismatch":
                throw NativeLocalAgentAttachmentVaultError.integrityMismatch
            case "attachment_workspace_unavailable":
                throw NativeLocalAgentAttachmentWorkspaceError.unavailable
            case "attachment_workspace_conflict":
                throw NativeLocalAgentAttachmentWorkspaceError.conflictingFile
            default:
                throw NativeLocalAgentAttachmentVaultError.invalidAttachment
            }
        }
    }

    private func arguments(
        _ record: LocalAgentConversationAttachmentRecord,
        ownerUserID: String,
        conversationID: String
    ) throws -> [String: NativeJSONValue] {
        guard record.conversationID == conversationID,
              record.authorizedLocalRef.hasPrefix(Self.referencePrefix),
              record.byteSize > 0,
              record.byteSize <= Self.maximumAttachmentBytes,
              record.sha256.count == 64 else {
            throw NativeLocalAgentAttachmentVaultError.invalidAttachment
        }
        let token = String(record.authorizedLocalRef.dropFirst(Self.referencePrefix.count))
        guard let uuid = UUID(uuidString: token), uuid.uuidString.lowercased() == token else {
            throw NativeLocalAgentAttachmentVaultError.invalidReference
        }
        return [
            "owner_user_id": .string(ownerUserID), "conversation_id": .string(conversationID),
            "token": .string(token), "display_name": .string(record.displayName),
            "media_type": .string(record.mediaType),
            "byte_size": .number(Double(record.byteSize)),
            "sha256": .string(record.sha256.lowercased()),
        ]
    }

    private func scopedDirectory(ownerUserID: String, conversationID: String) -> URL {
        rootURL
            .appendingPathComponent(Self.scopeComponent(ownerUserID), isDirectory: true)
            .appendingPathComponent(Self.scopeComponent(conversationID), isDirectory: true)
    }

    private static func scopeComponent(_ value: String) -> String { sha256(Data(value.utf8)) }

    private static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

}

enum NativeLocalAgentAttachmentVaultError: LocalizedError, Equatable {
    case invalidAttachment
    case invalidReference
    case integrityMismatch

    var errorDescription: String? {
        switch self {
        case .invalidAttachment: "The local attachment is unavailable or exceeds its safety limit."
        case .invalidReference: "The local attachment reference is invalid."
        case .integrityMismatch: "The local attachment failed its integrity check."
        }
    }
}

private enum NativeLocalAgentAttachmentWorkerShapeError: Error { case invalid }

private extension NativeJSONValue {
    func attachmentObject() throws -> [String: NativeJSONValue] {
        guard case let .object(value) = self else {
            throw NativeLocalAgentAttachmentWorkerShapeError.invalid
        }
        return value
    }
}

private extension Dictionary where Key == String, Value == NativeJSONValue {
    func attachmentString(_ key: String) throws -> String {
        guard case let .string(value)? = self[key] else {
            throw NativeLocalAgentAttachmentWorkerShapeError.invalid
        }
        return value
    }

    func attachmentUInt64(_ key: String) throws -> UInt64 {
        guard case let .number(value)? = self[key], value.isFinite,
              value >= 0, value.rounded() == value,
              value <= 9_007_199_254_740_991 else {
            throw NativeLocalAgentAttachmentWorkerShapeError.invalid
        }
        return UInt64(value)
    }

    func attachmentOptionalUInt64(_ key: String) -> UInt64? {
        guard case let .number(value)? = self[key], value.isFinite,
              value >= 0, value.rounded() == value,
              value <= 9_007_199_254_740_991 else {
            return nil
        }
        return UInt64(value)
    }

    func attachmentUInt64String(_ key: String) throws -> UInt64 {
        guard let value = try? attachmentString(key), let parsed = UInt64(value) else {
            throw NativeLocalAgentAttachmentWorkerShapeError.invalid
        }
        return parsed
    }
}
