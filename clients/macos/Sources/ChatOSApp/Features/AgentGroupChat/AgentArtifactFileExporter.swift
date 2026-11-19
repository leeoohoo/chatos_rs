import ChatOSCore
import Foundation

enum AgentArtifactFileExporter {
    static let maximumBytes = 2 * 1_024 * 1_024

    static func export(
        _ reference: AgentArtifactFileReference,
        to destinationURL: URL
    ) async throws {
        guard reference.fileURL.isFileURL,
              destinationURL.isFileURL,
              reference.size > 0,
              reference.size <= maximumBytes,
              reference.sha256.utf8.count == 64 else {
            throw AgentArtifactFileExportError.invalidSource
        }
        do {
            try await AppCancellableDetachedWork.run {
                try AppVerifiedFileExporter.export(
                    sourceURL: reference.fileURL,
                    destinationURL: destinationURL,
                    expectedBytes: reference.size,
                    expectedSHA256: reference.sha256.lowercased(),
                    temporaryPrefix: ".chatos-artifact-export-"
                )
            }
        } catch is CancellationError {
            throw CancellationError()
        } catch AppVerifiedFileExporter.VerificationError.invalidSource {
            throw AgentArtifactFileExportError.invalidSource
        }
    }
}

enum AgentArtifactFileExportError: LocalizedError {
    case invalidSource

    var errorDescription: String? {
        "本地文档文件与记录的大小或 SHA-256 不一致。"
    }
}
