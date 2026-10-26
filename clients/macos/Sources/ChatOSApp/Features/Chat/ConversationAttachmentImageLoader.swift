import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
import ChatOSMediaWorkerClient
import ChatOSMediaWorkerProtocol
import Foundation

enum ConversationAttachmentImageLoader {
    static let maximumBytes = 20 * 1_024 * 1_024
    static let maximumSourcePixelCount = 64_000_000

    static func load(
        url: URL,
        sha256: String?,
        expectedBytes: Int,
        ownerUserID: String,
        maximumDisplayPixelSize: Int = 2_048,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil,
        mediaWorker: any MediaWorkerImageNormalizing = MediaWorkerProcessClient.shared,
        codeReadWorker: any CodeReadWorkerExecuting = CodeReadWorkerProcessClient.shared
    ) async throws -> AppDecodedThumbnail {
        let verified = try await verifySource(
            url: url,
            sha256: sha256,
            expectedBytes: expectedBytes,
            codeReadWorker: codeReadWorker
        )
        let decoded = try await AppImageThumbnailLoader.loadLocalFileUsingWorker(
            url,
            sourceIdentity: verified.workerIdentity,
            ownerUserID: ownerUserID,
            maximumBytes: maximumBytes,
            maximumSourcePixelCount: maximumSourcePixelCount,
            maximumDisplayPixelSize: maximumDisplayPixelSize,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride,
            mediaWorker: mediaWorker
        )
        let final = try await verifySource(
            url: url,
            sha256: sha256,
            expectedBytes: expectedBytes,
            codeReadWorker: codeReadWorker
        )
        guard final.workerIdentity == verified.workerIdentity,
              final.modifiedSeconds == verified.modifiedSeconds,
              final.modifiedNanoseconds == verified.modifiedNanoseconds else {
            throw CocoaError(.fileReadCorruptFile)
        }
        return decoded
    }

    private struct VerifiedSource: Sendable {
        let workerIdentity: MediaWorkerFileIdentity
        let modifiedSeconds: Int64
        let modifiedNanoseconds: Int64
    }

    private static func verifySource(
        url: URL,
        sha256: String?,
        expectedBytes: Int,
        codeReadWorker: any CodeReadWorkerExecuting
    ) async throws -> VerifiedSource {
        guard url.isFileURL,
              expectedBytes > 0,
              expectedBytes <= maximumBytes,
              let sha256,
              sha256.utf8.count == 64,
              sha256.utf8.allSatisfy({
                  (48...57).contains($0) || (65...70).contains($0) || (97...102).contains($0)
              }),
              !url.lastPathComponent.isEmpty else {
            throw CocoaError(.fileReadCorruptFile)
        }
        let result = try await codeReadWorker.execute(
            operation: "verify_file_identity_v1",
            arguments: [
                "name": .string(url.lastPathComponent),
                "byte_size": .number(Double(expectedBytes)),
                "sha256": .string(sha256.lowercased()),
            ],
            projectRoot: url.deletingLastPathComponent(),
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepURL: nil
        )
        guard case let .object(fields) = result,
              case let .string(deviceString)? = fields["device"],
              let device = UInt64(deviceString),
              case let .string(inodeString)? = fields["inode"],
              let inode = UInt64(inodeString),
              case let .number(byteCountValue)? = fields["byte_count"],
              byteCountValue == Double(expectedBytes),
              case let .string(modifiedSecondsString)? = fields["modified_seconds"],
              let modifiedSeconds = Int64(modifiedSecondsString),
              case let .string(modifiedNanosecondsString)? = fields["modified_nanoseconds"],
              let modifiedNanoseconds = Int64(modifiedNanosecondsString) else {
            throw CocoaError(.fileReadCorruptFile)
        }
        return .init(
            workerIdentity: .init(
                device: device,
                inode: inode,
                byteCount: Int64(expectedBytes)
            ),
            modifiedSeconds: modifiedSeconds,
            modifiedNanoseconds: modifiedNanoseconds
        )
    }
}
