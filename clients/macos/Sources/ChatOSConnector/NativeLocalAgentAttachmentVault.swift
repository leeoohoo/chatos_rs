import ChatOSCore
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
            "display_name": .string(displayName),
            "media_type": .string(mediaType),
            "byte_size": .number(Double(byteSize)),
            "sha256": .string(sha256),
            "offset": .number(Double(offset)),
            "next_offset": nextOffset.map { .number(Double($0)) } ?? .null,
            "encoding": .string(encoding),
            "content": .string(content),
        ])
    }
}

struct NativeLocalAgentAttachmentVault: Sendable {
    static let referencePrefix = "local-attachment:"
    static let maximumAttachmentBytes = 20 * 1_024 * 1_024
    static let maximumReadBytes = 64 * 1_024

    let rootURL: URL
    private let integrityCache = NativeLocalAgentAttachmentIntegrityCache()

    func authorize(
        _ drafts: [ConversationAttachmentDraft],
        ownerUserID: String,
        conversationID: String
    ) throws -> [LocalAgentConversationAttachmentSpec] {
        try drafts.map { draft in
            guard !draft.data.isEmpty, draft.data.count <= Self.maximumAttachmentBytes else {
                throw NativeLocalAgentAttachmentVaultError.invalidAttachment
            }
            let token = UUID().uuidString.lowercased()
            let directory = scopedDirectory(
                ownerUserID: ownerUserID,
                conversationID: conversationID
            )
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            let file = directory.appendingPathComponent(token, isDirectory: false)
            try draft.data.write(to: file, options: .atomic)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: file.path
            )
            return .init(
                attachmentID: draft.id,
                displayName: draft.name,
                mediaType: draft.mimeType,
                byteSize: UInt64(draft.data.count),
                sha256: Self.sha256(draft.data),
                authorizedLocalRef: Self.referencePrefix + token,
                metadata: .object(["kind": .string(draft.kind.rawValue)])
            )
        }
    }

    func resolve(
        _ record: LocalAgentConversationAttachmentRecord,
        ownerUserID: String,
        conversationID: String,
        offset: UInt64,
        limit: Int
    ) async throws -> NativeLocalAgentResolvedAttachment {
        try await Task.detached(priority: .utility) {
            try resolveSync(
                record,
                ownerUserID: ownerUserID,
                conversationID: conversationID,
                offset: offset,
                limit: limit
            )
        }.value
    }

    private func resolveSync(
        _ record: LocalAgentConversationAttachmentRecord,
        ownerUserID: String,
        conversationID: String,
        offset: UInt64,
        limit: Int
    ) throws -> NativeLocalAgentResolvedAttachment {
        guard (1...Self.maximumReadBytes).contains(limit),
              offset <= record.byteSize else {
            throw NativeLocalAgentAttachmentVaultError.invalidAttachment
        }
        let file = try validatedFileURL(
            record,
            ownerUserID: ownerUserID,
            conversationID: conversationID
        )
        let handle = try FileHandle(forReadingFrom: file)
        defer { try? handle.close() }
        let signature = try Self.signature(
            fileDescriptor: handle.fileDescriptor,
            sha256: record.sha256.lowercased()
        )
        guard signature.byteSize == record.byteSize else {
            throw NativeLocalAgentAttachmentVaultError.integrityMismatch
        }
        if !integrityCache.contains(signature) {
            try handle.seek(toOffset: 0)
            let data = try handle.readToEnd() ?? Data()
            guard data.count == Int(record.byteSize),
                  data.count <= Self.maximumAttachmentBytes,
                  Self.sha256(data) == record.sha256.lowercased() else {
                throw NativeLocalAgentAttachmentVaultError.integrityMismatch
            }
            integrityCache.insert(signature)
        }
        let expectedCount = min(limit, Int(record.byteSize - offset))
        try handle.seek(toOffset: offset)
        let slice = expectedCount == 0
            ? Data()
            : try handle.read(upToCount: expectedCount) ?? Data()
        guard slice.count == expectedCount,
              try Self.signature(
                fileDescriptor: handle.fileDescriptor,
                sha256: record.sha256.lowercased()
              ) == signature else {
            throw NativeLocalAgentAttachmentVaultError.integrityMismatch
        }
        let end = offset + UInt64(slice.count)
        let text = String(data: slice, encoding: .utf8)
        let encoding = text == nil ? "base64" : "utf-8"
        let content = text ?? slice.base64EncodedString()
        return .init(
            displayName: record.displayName,
            mediaType: record.mediaType,
            byteSize: record.byteSize,
            sha256: record.sha256.lowercased(),
            offset: offset,
            nextOffset: end < record.byteSize ? end : nil,
            encoding: encoding,
            content: content
        )
    }

    func previewURL(
        _ record: LocalAgentConversationAttachmentRecord,
        ownerUserID: String,
        conversationID: String
    ) throws -> URL {
        try validatedFileURL(
            record,
            ownerUserID: ownerUserID,
            conversationID: conversationID
        )
    }

    private func validatedFileURL(
        _ record: LocalAgentConversationAttachmentRecord,
        ownerUserID: String,
        conversationID: String
    ) throws -> URL {
        guard record.authorizedLocalRef.hasPrefix(Self.referencePrefix),
              record.byteSize > 0,
              record.byteSize <= Self.maximumAttachmentBytes,
              record.sha256.count == 64 else {
            throw NativeLocalAgentAttachmentVaultError.invalidAttachment
        }
        let token = String(record.authorizedLocalRef.dropFirst(Self.referencePrefix.count))
        guard let uuid = UUID(uuidString: token),
              uuid.uuidString.lowercased() == token else {
            throw NativeLocalAgentAttachmentVaultError.invalidReference
        }
        let directory = scopedDirectory(
            ownerUserID: ownerUserID,
            conversationID: conversationID
        )
        let file = directory.appendingPathComponent(token, isDirectory: false)
        let values = try file.resourceValues(forKeys: [
            .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
        ])
        guard values.isRegularFile == true,
              values.isSymbolicLink != true,
              values.fileSize == Int(record.byteSize),
              Self.isContained(file: file, in: directory) else {
            throw NativeLocalAgentAttachmentVaultError.invalidAttachment
        }
        return file
    }

    private func scopedDirectory(ownerUserID: String, conversationID: String) -> URL {
        rootURL
            .appendingPathComponent(Self.scopeComponent(ownerUserID), isDirectory: true)
            .appendingPathComponent(Self.scopeComponent(conversationID), isDirectory: true)
    }

    private static func scopeComponent(_ value: String) -> String {
        sha256(Data(value.utf8))
    }

    private static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    private static func signature(
        fileDescriptor: Int32,
        sha256: String
    ) throws -> NativeLocalAgentAttachmentFileSignature {
        var metadata = stat()
        guard fstat(fileDescriptor, &metadata) == 0,
              metadata.st_size >= 0 else {
            throw NativeLocalAgentAttachmentVaultError.invalidAttachment
        }
        return .init(
            device: UInt64(metadata.st_dev),
            inode: UInt64(metadata.st_ino),
            byteSize: UInt64(metadata.st_size),
            modifiedSeconds: Int64(metadata.st_mtimespec.tv_sec),
            modifiedNanoseconds: Int64(metadata.st_mtimespec.tv_nsec),
            changedSeconds: Int64(metadata.st_ctimespec.tv_sec),
            changedNanoseconds: Int64(metadata.st_ctimespec.tv_nsec),
            sha256: sha256
        )
    }

    private static func isContained(file: URL, in directory: URL) -> Bool {
        let parent = file.deletingLastPathComponent().resolvingSymlinksInPath().standardizedFileURL
        let expected = directory.resolvingSymlinksInPath().standardizedFileURL
        return parent == expected
    }
}

private struct NativeLocalAgentAttachmentFileSignature: Hashable, Sendable {
    let device: UInt64
    let inode: UInt64
    let byteSize: UInt64
    let modifiedSeconds: Int64
    let modifiedNanoseconds: Int64
    let changedSeconds: Int64
    let changedNanoseconds: Int64
    let sha256: String
}

private final class NativeLocalAgentAttachmentIntegrityCache: @unchecked Sendable {
    private static let maximumEntries = 256
    private let lock = NSLock()
    private var entries: Set<NativeLocalAgentAttachmentFileSignature> = []

    func contains(_ signature: NativeLocalAgentAttachmentFileSignature) -> Bool {
        lock.withLock { entries.contains(signature) }
    }

    func insert(_ signature: NativeLocalAgentAttachmentFileSignature) {
        lock.withLock {
            if entries.count >= Self.maximumEntries {
                entries.removeAll(keepingCapacity: true)
            }
            entries.insert(signature)
        }
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
