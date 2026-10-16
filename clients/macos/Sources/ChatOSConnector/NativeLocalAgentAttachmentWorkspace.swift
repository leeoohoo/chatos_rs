import ChatOSCore
import CryptoKit
import Darwin
import Foundation

protocol NativeLocalAgentAttachmentWorkspaceResolving: Sendable {
    func attachmentWorkspaceRoot(ownerUserID: String, runID: String) async throws -> URL
}

extension NativeLocalAgentProjectContextResolver: NativeLocalAgentAttachmentWorkspaceResolving {
    func attachmentWorkspaceRoot(ownerUserID: String, runID: String) async throws -> URL {
        try await resolve(ownerUserID: ownerUserID, runID: runID).executionRootURL
    }
}

struct NativeLocalAgentWorkspaceAttachment: Sendable {
    let relativePath: String
    let record: LocalAgentConversationAttachmentRecord

    var jsonValue: LocalAgentJSONValue {
        .object([
            "delivery": .string("workspace"),
            "workspace_relative_path": .string(relativePath),
            "display_name": .string(record.displayName),
            "media_type": .string(record.mediaType),
            "byte_size": .number(Double(record.byteSize)),
            "sha256": .string(record.sha256.lowercased()),
            "sha256_verified": .bool(true),
        ])
    }
}

enum NativeLocalAgentAttachmentWorkspace {
    static func materialize(
        _ data: Data,
        record: LocalAgentConversationAttachmentRecord,
        ownerUserID: String,
        conversationID: String,
        rootURL: URL
    ) async throws -> NativeLocalAgentWorkspaceAttachment {
        try await Task.detached(priority: .utility) {
            guard record.conversationID == conversationID,
                  record.byteSize > 0,
                  record.byteSize <= NativeLocalAgentAttachmentVault.maximumAttachmentBytes,
                  data.count == Int(record.byteSize),
                  data.count <= NativeLocalAgentAttachmentVault.maximumAttachmentBytes,
                  hash(data) == record.sha256.lowercased() else {
                throw NativeLocalAgentAttachmentVaultError.integrityMismatch
            }
            let components = [
                ".chatos-attachments",
                NativePluginManifestLoader.sha256(ownerUserID + "\n" + conversationID),
                NativePluginManifestLoader.sha256(record.authorizedLocalRef),
            ]
            let rawExtension = (record.displayName as NSString).pathExtension.lowercased()
            let safeExtension = !rawExtension.isEmpty && rawExtension.utf8.count <= 12
                && rawExtension.utf8.allSatisfy { (97...122).contains($0) || (48...57).contains($0) }
            let name = "source" + (safeExtension ? "." + rawExtension : "")
            // Pin each directory with openat/O_NOFOLLOW. A project may contain
            // hostile symlinks; never follow them or overwrite an existing file.
            let root = open(rootURL.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
            guard root >= 0 else { throw NativeLocalAgentAttachmentWorkspaceError.unavailable }
            var descriptors = [root]
            defer { descriptors.reversed().forEach { _ = close($0) } }
            for component in components {
                let parent = descriptors.last!
                if mkdirat(parent, component, 0o700) != 0 && errno != EEXIST {
                    throw NativeLocalAgentAttachmentWorkspaceError.unavailable
                }
                let child = openat(parent, component, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
                guard child >= 0 else { throw NativeLocalAgentAttachmentWorkspaceError.unavailable }
                descriptors.append(child)
            }
            let directory = descriptors.last!
            let temporaryName = ".import-" + UUID().uuidString.lowercased()
            let temporary = openat(directory, temporaryName, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0o600)
            guard temporary >= 0 else { throw NativeLocalAgentAttachmentWorkspaceError.unavailable }
            defer { _ = close(temporary); _ = unlinkat(directory, temporaryName, 0) }
            try data.withUnsafeBytes { bytes in
                var written = 0
                while written < bytes.count {
                    let count = Darwin.write(temporary, bytes.baseAddress!.advanced(by: written), bytes.count - written)
                    if count < 0 && errno == EINTR { continue }
                    guard count > 0 else { throw NativeLocalAgentAttachmentWorkspaceError.unavailable }
                    written += count
                }
            }
            guard fsync(temporary) == 0 else { throw NativeLocalAgentAttachmentWorkspaceError.unavailable }
            // linkat installs atomically with no replacement, so concurrent reads
            // share one verified source copy and a user-edited copy is not lost.
            if linkat(directory, temporaryName, directory, name, 0) != 0 && errno != EEXIST {
                throw NativeLocalAgentAttachmentWorkspaceError.unavailable
            }
            let installed = openat(directory, name, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
            guard installed >= 0 else { throw NativeLocalAgentAttachmentWorkspaceError.conflictingFile }
            let handle = FileHandle(fileDescriptor: installed, closeOnDealloc: true)
            defer { try? handle.close() }
            var metadata = stat()
            guard fstat(installed, &metadata) == 0,
                  metadata.st_mode & S_IFMT == S_IFREG,
                  metadata.st_size == Int64(data.count),
                  let installedBytes = try handle.read(upToCount: data.count + 1),
                  installedBytes == data else {
                throw NativeLocalAgentAttachmentWorkspaceError.conflictingFile
            }
            return .init(relativePath: (components + [name]).joined(separator: "/"), record: record)
        }.value
    }

    private static func hash(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}

enum NativeLocalAgentAttachmentWorkspaceError: LocalizedError, Equatable {
    case unavailable
    case conflictingFile

    var errorDescription: String? {
        switch self {
        case .unavailable: "The authorized attachment could not be imported into the task workspace."
        case .conflictingFile: "The workspace attachment copy has changed or is unsafe; it was not overwritten."
        }
    }
}
