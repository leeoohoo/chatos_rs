import Foundation
import Testing
import AppKit
import ChatOSCore
@testable import ChatOSApp

@Suite("App Bounded File Reader")
struct AppBoundedFileReaderTests {
    @Test("reads a regular file below the configured limit")
    func readsBoundedFile() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("app-bounded-file-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        let expected = Data("bounded".utf8)
        try expected.write(to: url)

        #expect(try AppBoundedFileReader.read(url, maximumBytes: 64) == expected)
    }

    @Test("rejects an oversized sparse file before loading contents")
    func rejectsOversizedFile() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("app-bounded-file-large-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        #expect(FileManager.default.createFile(atPath: url.path, contents: nil))
        let handle = try FileHandle(forWritingTo: url)
        try handle.truncate(atOffset: 1_025)
        try handle.close()

        #expect(throws: AppBoundedFileReadError.self) {
            _ = try AppBoundedFileReader.read(url, maximumBytes: 1_024)
        }
    }

    @Test("stops a bounded read when the calling task is already cancelled")
    func respectsCancellation() async throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("app-bounded-file-cancelled-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        try Data(repeating: 7, count: 512 * 1_024).write(to: url)

        let task = Task.detached { () throws -> Data in
            withUnsafeCurrentTask { $0?.cancel() }
            return try AppBoundedFileReader.read(url, maximumBytes: 1_024 * 1_024)
        }

        await #expect(throws: CancellationError.self) {
            _ = try await task.value
        }
    }

    @Test("cancelling a detached work waiter cancels its background operation")
    func cancellableDetachedWorkPropagatesCancellation() async throws {
        let probe = DetachedWorkProbe()
        let waiter = Task {
            try await AppCancellableDetachedWork.run {
                try probe.runUntilCancelled()
            }
        }
        for _ in 0..<200 where probe.snapshot().starts == 0 {
            try await Task.sleep(for: .milliseconds(5))
        }
        #expect(probe.snapshot().starts == 1)

        waiter.cancel()
        await #expect(throws: CancellationError.self) {
            _ = try await waiter.value
        }
        for _ in 0..<200 where probe.snapshot().cancellations == 0 {
            try await Task.sleep(for: .milliseconds(5))
        }
        #expect(probe.snapshot().cancellations == 1)
    }

    @Test("agent image loader verifies the recorded size with a bounded background read")
    func loadsAgentImageWithExactRecordedSize() async throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-image-bounded-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        let expected = Data("image-bytes".utf8)
        try expected.write(to: url)
        let payload = ProjectAgentMessageAttachmentPayload(
            attachment: .init(
                id: "attachment-1",
                name: "image.png",
                mimeType: "image/png",
                size: expected.count,
                kind: .image,
                origin: .file
            ),
            localFileURL: url
        )

        #expect(try await AgentMessageAttachmentDataLoader.load(payload) == expected)
    }

    @Test("agent image loader rejects metadata above the attachment limit before reading")
    func rejectsOversizedAgentImageMetadata() async throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-image-oversized-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        try Data("tiny".utf8).write(to: url)
        let payload = ProjectAgentMessageAttachmentPayload(
            attachment: .init(
                id: "attachment-2",
                name: "image.png",
                mimeType: "image/png",
                size: AgentMessageAttachmentDataLoader.maximumBytes + 1,
                kind: .image,
                origin: .file
            ),
            localFileURL: url
        )

        do {
            _ = try await AgentMessageAttachmentDataLoader.load(payload)
            Issue.record("Oversized attachment metadata was accepted")
        } catch is AgentAttachmentPresentationError {
            // Expected.
        }
    }

    @Test("image thumbnail loader decodes and downsamples away from the UI")
    func decodesBoundedImageThumbnail() async throws {
        let data = try png(width: 64, height: 32)
        let decoded = try #require(AppImageThumbnailLoader.decode(
            data,
            maximumSourcePixelCount: 64 * 32,
            maximumDisplayPixelSize: 16
        ))

        #expect(decoded.image.width <= 16)
        #expect(decoded.image.height <= 16)
        #expect(decoded.memoryCost > 0)
    }

    @Test("image thumbnail loader rejects an image above the source pixel budget")
    func rejectsImageAbovePixelBudget() throws {
        let data = try png(width: 64, height: 32)
        #expect(AppImageThumbnailLoader.decode(
            data,
            maximumSourcePixelCount: (64 * 32) - 1,
            maximumDisplayPixelSize: 16
        ) == nil)
    }

    @Test("local image thumbnail loader rejects a file above the byte budget")
    func rejectsImageAboveByteBudget() async throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("app-image-large-\(UUID().uuidString).png")
        defer { try? FileManager.default.removeItem(at: url) }
        try Data(repeating: 0, count: 1_025).write(to: url)

        await #expect(throws: AppBoundedFileReadError.self) {
            _ = try await AppImageThumbnailLoader.loadLocalFile(
                url,
                maximumBytes: 1_024,
                maximumSourcePixelCount: 1_000_000,
                maximumDisplayPixelSize: 256
            )
        }
    }

    @MainActor
    @Test("base64 image cache decodes and bounds an inline image asynchronously")
    func decodesBase64ImageThroughSharedCache() async throws {
        let data = try png(width: 64, height: 32)
        let base64 = data.base64EncodedString()
        let key = AppImageDataThumbnailCache.base64Identity(
            base64,
            identity: "inline-test",
            maximumSourcePixelCount: 64 * 32,
            maximumDisplayPixelSize: 16
        )
        let image = await AppImageDataThumbnailCache.image(
            forBase64: base64,
            cacheKey: key,
            maximumEncodedCharacters: base64.count,
            maximumSourcePixelCount: 64 * 32,
            maximumDisplayPixelSize: 16
        )

        #expect(image != nil)
        #expect((image?.representations.first?.pixelsWide ?? 0) <= 16)
        #expect((image?.representations.first?.pixelsHigh ?? 0) <= 16)
    }

    @MainActor
    @Test("base64 image cache rejects encoded input above its character budget")
    func rejectsOversizedBase64ImageBeforeDecode() async {
        let base64 = String(repeating: "A", count: 1_025)
        let key = AppImageDataThumbnailCache.base64Identity(
            base64,
            identity: "oversized-inline-test",
            maximumSourcePixelCount: 1_000_000,
            maximumDisplayPixelSize: 256
        )
        let image = await AppImageDataThumbnailCache.image(
            forBase64: base64,
            cacheKey: key,
            maximumEncodedCharacters: 1_024,
            maximumSourcePixelCount: 1_000_000,
            maximumDisplayPixelSize: 256
        )

        #expect(image == nil)
    }

    @MainActor
    @Test("shared thumbnail work stops only after its last waiter is cancelled")
    func sharedThumbnailWorkUsesWaiterAwareCancellation() async throws {
        let pool = AppSharedThumbnailTaskPool()
        let probe = ThumbnailTaskProbe()
        let operation: @Sendable () async -> AppDecodedThumbnail? = {
            await probe.started()
            do {
                try await Task.sleep(for: .seconds(30))
            } catch {
                await probe.cancelled()
            }
            return nil
        }

        let first = Task { @MainActor in
            await pool.value(for: "shared-test", operation: operation)
        }
        let second = Task { @MainActor in
            await pool.value(for: "shared-test", operation: operation)
        }
        try await waitForProbe(probe) { $0.starts == 1 }

        first.cancel()
        try await Task.sleep(for: .milliseconds(30))
        #expect(await probe.snapshot().cancellations == 0)

        second.cancel()
        _ = await first.value
        _ = await second.value
        try await waitForProbe(probe) { $0.cancellations == 1 }
        #expect(await probe.snapshot().starts == 1)
    }

    @MainActor
    private func waitForProbe(
        _ probe: ThumbnailTaskProbe,
        condition: (ThumbnailTaskProbe.Snapshot) -> Bool
    ) async throws {
        for _ in 0..<200 {
            if condition(await probe.snapshot()) { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        Issue.record("Timed out waiting for shared thumbnail task state")
    }

    private func png(width: Int, height: Int) throws -> Data {
        let bitmap = try #require(NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: width,
            pixelsHigh: height,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ))
        bitmap.bitmapData?.initialize(
            repeating: 127,
            count: bitmap.bytesPerRow * bitmap.pixelsHigh
        )
        return try #require(bitmap.representation(using: .png, properties: [:]))
    }
}

private actor ThumbnailTaskProbe {
    struct Snapshot: Sendable {
        let starts: Int
        let cancellations: Int
    }

    private var starts = 0
    private var cancellations = 0

    func started() { starts += 1 }
    func cancelled() { cancellations += 1 }
    func snapshot() -> Snapshot { Snapshot(starts: starts, cancellations: cancellations) }
}

private final class DetachedWorkProbe: @unchecked Sendable {
    struct Snapshot {
        let starts: Int
        let cancellations: Int
    }

    private let lock = NSLock()
    private var starts = 0
    private var cancellations = 0

    func runUntilCancelled() throws -> Int {
        lock.withLock { starts += 1 }
        while true {
            do {
                try Task.checkCancellation()
                Thread.sleep(forTimeInterval: 0.005)
            } catch {
                lock.withLock { cancellations += 1 }
                throw error
            }
        }
    }

    func snapshot() -> Snapshot {
        lock.withLock { Snapshot(starts: starts, cancellations: cancellations) }
    }
}
