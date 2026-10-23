import ChatOSCore
import Foundation

/// Creates a lease-owned media artifact through a securely pre-opened file descriptor.
public enum ChatOSMediaArtifactFileWriter {
    public static func writeDownloadedImage(
        _ body: AsyncThrowingStream<Data, Error>,
        maximumBytes: Int,
        expectedBytes: Int? = nil,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil
    ) async throws -> GeneratedMediaFile {
        try await ChatOSResponseBodyFileWriter.write(
            body,
            maximumBytes: maximumBytes,
            expectedBytes: expectedBytes,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride,
            artifactKind: .imageResponse
        )
    }

    public static func writeNormalizedImage(
        maximumBytes: Int,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil,
        populate: @escaping @Sendable (FileHandle) throws -> Void
    ) async throws -> GeneratedMediaFile {
        let task = Task.detached(priority: .userInitiated) {
            try ChatOSResponseBodyFileWriter.writeUsingFileDescriptor(
                maximumBytes: maximumBytes,
                temporaryDirectory: temporaryDirectory,
                availableCapacityOverride: availableCapacityOverride,
                artifactKind: .normalizedImage,
                populate: populate
            )
        }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
    }

    public static func writeNormalizedImageUsingWorker(
        maximumBytes: Int,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil,
        populate: @escaping @Sendable (URL, FileHandle) async throws -> Void
    ) async throws -> GeneratedMediaFile {
        try await ChatOSResponseBodyFileWriter.writeUsingFileDescriptorAsync(
            maximumBytes: maximumBytes,
            temporaryDirectory: temporaryDirectory,
            availableCapacityOverride: availableCapacityOverride,
            artifactKind: .normalizedImage,
            populate: populate
        )
    }
}
