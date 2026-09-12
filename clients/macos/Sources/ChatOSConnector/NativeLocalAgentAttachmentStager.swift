// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public enum NativeLocalAgentAttachmentStagingError: Error, Equatable, Sendable {
    case invalidAttachment(String)
    case attachmentTooLarge(String)
    case totalPayloadTooLarge
    case privateGrantDirectoryRequired
    case writeFailed(String)
}

extension NativeLocalAgentAttachmentStagingError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case let .invalidAttachment(name):
            "附件“\(name)”无效"
        case let .attachmentTooLarge(name):
            "附件“\(name)”超过 5 MB"
        case .totalPayloadTooLarge:
            "本轮附件总大小不能超过 6 MB"
        case .privateGrantDirectoryRequired:
            "本地 Agent 附件授权目录不可用"
        case let .writeFailed(name):
            "无法暂存附件“\(name)”"
        }
    }
}

/// Copies UI-owned bytes into the account-private grant directory and exposes
/// only opaque grant identities to IPC. The Host independently verifies file
/// type, size and SHA-256 before any bytes enter a model request.
public struct NativeLocalAgentAttachmentStager: Sendable {
    public static let maximumAttachmentBytes = 5 * 1_024 * 1_024
    public static let maximumTotalBytes = 6 * 1_024 * 1_024

    public init() {}

    public func stage(
        _ attachments: [ConversationAttachmentDraft],
        in grantDirectory: URL
    ) throws -> [LocalAgentAttachmentReference] {
        do {
            try NativeLocalAgentHostBootstrapBuilder.ensurePrivateDirectory(grantDirectory)
        } catch {
            throw NativeLocalAgentAttachmentStagingError.privateGrantDirectoryRequired
        }

        var totalBytes = 0
        var attachmentIDs = Set<String>()
        for attachment in attachments {
            guard validIdentity(attachment.id), validMediaType(attachment.mimeType),
                  !attachment.data.isEmpty
            else {
                throw NativeLocalAgentAttachmentStagingError.invalidAttachment(attachment.name)
            }
            guard attachmentIDs.insert(attachment.id).inserted else {
                throw NativeLocalAgentAttachmentStagingError.invalidAttachment(attachment.name)
            }
            guard attachment.data.count <= Self.maximumAttachmentBytes else {
                throw NativeLocalAgentAttachmentStagingError.attachmentTooLarge(attachment.name)
            }
            let (nextTotal, overflow) = totalBytes.addingReportingOverflow(attachment.data.count)
            guard !overflow, nextTotal <= Self.maximumTotalBytes else {
                throw NativeLocalAgentAttachmentStagingError.totalPayloadTooLarge
            }
            totalBytes = nextTotal
        }

        var stagedURLs: [URL] = []
        do {
            let references = try attachments.map { attachment in
                let grantID = "grant-\(UUID().uuidString.lowercased())"
                let destination = grantDirectory.appendingPathComponent(
                    "\(grantID).payload",
                    isDirectory: false
                )
                do {
                    try attachment.data.write(to: destination, options: .withoutOverwriting)
                    try FileManager.default.setAttributes(
                        [.posixPermissions: 0o600],
                        ofItemAtPath: destination.path
                    )
                } catch {
                    try? FileManager.default.removeItem(at: destination)
                    throw NativeLocalAgentAttachmentStagingError.writeFailed(attachment.name)
                }
                stagedURLs.append(destination)
                return LocalAgentAttachmentReference(
                    attachmentID: attachment.id,
                    mediaType: attachment.mimeType,
                    payloadReference: "attachment-grant:\(grantID)",
                    payloadDigest: "sha256:\(NativePluginHash.sha256(attachment.data))",
                    byteSize: UInt64(attachment.data.count)
                )
            }
            return references
        } catch {
            for url in stagedURLs {
                try? FileManager.default.removeItem(at: url)
            }
            throw error
        }
    }

    private func validIdentity(_ value: String) -> Bool {
        !value.isEmpty
            && value.count <= 512
            && value == value.trimmingCharacters(in: .whitespacesAndNewlines)
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }

    private func validMediaType(_ value: String) -> Bool {
        validIdentity(value)
            && value.utf8.count <= 255
            && value.contains("/")
            && !value.contains(where: \.isWhitespace)
    }
}
