import ChatOSCore
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
