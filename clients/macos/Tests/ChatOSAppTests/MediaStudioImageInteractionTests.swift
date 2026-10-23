import AppKit
import ChatOSAPI
import ChatOSCore
import ChatOSMediaWorkerClient
import ChatOSMediaWorkerCore
import ChatOSMediaWorkerProtocol
import ChatOSNetworking
import Darwin
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class MediaStudioImageInteractionTests: XCTestCase {
    func testProcessIsolatedImageNormalizationDoesNotWaitForHeavyMediaLane() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MediaStudioGovernedNormalizationTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 64, pixelsHigh: 32,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let governor = ClientHeavyMediaWorkGovernor(limit: 1)
        let blocker = try await governor.acquire(context: .init(
            taskID: "blocker",
            kind: .videoFrameExtraction,
            estimatedDecodedBytes: 64 * 1_024 * 1_024
        ))
        let normalization = Task {
            try await MediaStudioImageLoader.normalizedPNGFile(
                for: .init(
                    id: "governed-normalization",
                    mimeType: "image/png",
                    base64Data: png.base64EncodedString()
                ),
                ownerUserID: "test-user",
                temporaryDirectory: root,
                availableCapacityOverride: Int64.max,
                mediaWorker: TestMediaWorkerImageNormalizer()
            )
        }
        try await Task.sleep(for: .milliseconds(25))
        let whileBlocked = await governor.snapshot()
        XCTAssertTrue(whileBlocked.queued.isEmpty)
        XCTAssertEqual(whileBlocked.active.map(\.taskID), ["blocker"])
        let normalized = try await normalization.value
        XCTAssertTrue(FileManager.default.fileExists(atPath: normalized.url.path))
        normalized.removeIfNeeded()
        await blocker.releaseAndWait()
        let completed = await governor.snapshot()
        XCTAssertTrue(completed.active.isEmpty)
        XCTAssertTrue(completed.queued.isEmpty)
    }

    func testBoundedSmallThumbnailDoesNotWaitForHeavyMediaLane() async throws {
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 64, pixelsHigh: 32,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let governor = ClientHeavyMediaWorkGovernor(limit: 1)
        let blocker = try await governor.acquire(context: .init(
            taskID: "blocker",
            kind: .videoFrameExtraction,
            estimatedDecodedBytes: 64 * 1_024 * 1_024
        ))

        let thumbnail = try await MediaStudioImageLoader.thumbnail(
            for: .init(
                id: "small-thumbnail",
                mimeType: "image/png",
                base64Data: png.base64EncodedString()
            ),
            ownerUserID: "test-user",
            maximumDisplayPixelSize: 64,
            heavyMediaGovernor: governor,
            mediaWorker: TestMediaWorkerImageNormalizer()
        )

        XCTAssertEqual(thumbnail.image.width, 64)
        let snapshot = await governor.snapshot()
        XCTAssertTrue(snapshot.queued.isEmpty)
        XCTAssertEqual(snapshot.active.map(\.taskID), ["blocker"])
        await blocker.releaseAndWait()
    }

    func testProcessIsolatedVideoFrameDoesNotWaitForHeavyMediaLane() async throws {
        let invalidVideo = FileManager.default.temporaryDirectory.appendingPathComponent(
            "governed-frame-\(UUID().uuidString).mp4"
        )
        try Data("not-a-video".utf8).write(to: invalidVideo)
        addTeardownBlock { try? FileManager.default.removeItem(at: invalidVideo) }
        let governor = ClientHeavyMediaWorkGovernor(limit: 1)
        let blocker = try await governor.acquire(context: .init(
            taskID: "blocker",
            kind: .imageDecode,
            estimatedDecodedBytes: 64 * 1_024 * 1_024
        ))
        let extraction = Task {
            try await StoryVideoFrameExtractor.lastFramePNG(
                from: invalidVideo,
                ownerUserID: "test-user",
                mediaWorker: RejectingVideoFrameWorker()
            )
        }
        try await Task.sleep(for: .milliseconds(25))
        let whileBlocked = await governor.snapshot()
        XCTAssertTrue(whileBlocked.queued.isEmpty)
        XCTAssertEqual(whileBlocked.active.map(\.taskID), ["blocker"])
        do {
            _ = try await extraction.value
            XCTFail("Invalid video unexpectedly produced a final frame")
        } catch {
            // Expected from the independent Worker; the fixture is intentionally not a video.
        }
        await blocker.releaseAndWait()
        let completed = await governor.snapshot()
        XCTAssertTrue(completed.active.isEmpty)
        XCTAssertTrue(completed.queued.isEmpty)
    }

    func testImageWorkspaceImportsLocalReferencesAndEnforcesCapacityBeforeReading() async throws {
        let (vm, _, upload) = try await fixture()

        vm.addInputImages(from: [upload])
        try await wait { !vm.isLoadingInputImages }
        XCTAssertEqual(vm.inputImages.map(\.name), ["upload.png"])
        XCTAssertTrue(vm.inputImages[0].base64Data.isEmpty)
        XCTAssertNotNil(vm.inputImages[0].fileURL)
        XCTAssertNil(vm.errorMessage)

        vm.addInputImages(from: Array(repeating: upload, count: 8))
        XCTAssertFalse(vm.isLoadingInputImages)
        XCTAssertEqual(vm.inputImages.map(\.name), ["upload.png"])
        XCTAssertEqual(vm.errorMessage, "一次最多选择 8 张参考图。")
    }

    func testSelectAnySavedImageAsVideoReferenceWithoutStartingGeneration() async throws {
        let (vm, service, _) = try await fixture()
        vm.prompt = "keep image prompt"
        vm.videoPrompt = "keep video prompt"
        let asset = vm.history[0].images[1]
        vm.useGeneratedImageForVideo(asset)
        XCTAssertEqual(vm.section, .video)
        XCTAssertTrue(vm.isLoadingVideoInputImage)
        XCTAssertFalse(vm.canGenerateVideo)
        try await wait { !vm.isLoadingVideoInputImage }
        let input = try XCTUnwrap(vm.videoInputImage)
        XCTAssertEqual(input.name, "generated-second.png")
        XCTAssertEqual(input.mimeType, "image/png")
        XCTAssertTrue(input.base64Data.isEmpty)
        XCTAssertNotNil(input.fileURL)
        XCTAssertEqual(vm.prompt, "keep image prompt")
        XCTAssertEqual(vm.videoPrompt, "keep video prompt")
        XCTAssertEqual(vm.history[0].images.count, 2)
        let before = await service.videoRequests
        XCTAssertTrue(before.isEmpty)
        vm.generateVideo()
        try await wait { !vm.isGeneratingVideo }
        let requests = await service.videoRequests
        XCTAssertEqual(requests.count, 1)
        let submittedInput = try XCTUnwrap(requests.first?.inputImage)
        XCTAssertEqual(submittedInput.name, input.name)
        XCTAssertEqual(submittedInput.mimeType, input.mimeType)
        XCTAssertTrue(submittedInput.base64Data.isEmpty)
        XCTAssertNotEqual(submittedInput.fileURL, input.fileURL)
        XCTAssertEqual(
            try Data(contentsOf: XCTUnwrap(submittedInput.fileURL)),
            try Data(contentsOf: XCTUnwrap(input.fileURL))
        )
        XCTAssertEqual(requests.first?.prompt, "keep video prompt")
    }

    func testLocalUploadCanReplaceGeneratedReferenceAndBeRemoved() async throws {
        let (vm, _, upload) = try await fixture()
        vm.useGeneratedImageForVideo(vm.history[0].images[0])
        try await wait { !vm.isLoadingVideoInputImage }
        vm.selectVideoInputImage(from: upload)
        try await wait { !vm.isLoadingVideoInputImage }
        XCTAssertEqual(vm.videoInputImage?.name, "upload.png")
        XCTAssertNil(vm.errorMessage)
        vm.removeVideoInputImage()
        XCTAssertNil(vm.videoInputImage)
        vm.useGeneratedImageForVideo(vm.history[0].images[1])
        try await wait { !vm.isLoadingVideoInputImage }
        XCTAssertEqual(vm.videoInputImage?.name, "generated-second.png")
    }

    func testRemovingPendingSelectionAndSwitchingAccountsIgnoreLateImageLoads() async throws {
        let (vm, _, _) = try await fixture()
        let asset = vm.history[0].images[0]
        vm.useGeneratedImageForVideo(asset)
        vm.removeVideoInputImage()
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertNil(vm.videoInputImage)
        XCTAssertFalse(vm.isLoadingVideoInputImage)
        vm.useGeneratedImageForVideo(asset)
        vm.activate(userID: "another-account")
        try await wait { !vm.isLoadingHistory }
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertNil(vm.videoInputImage)
        XCTAssertFalse(vm.isLoadingVideoInputImage)
        vm.useGeneratedImageForVideo(asset)
        XCTAssertFalse(vm.isLoadingVideoInputImage)
        XCTAssertNil(vm.videoInputImage)
    }

    func testMissingSavedImageReportsErrorWithoutReplacingExistingReference() async throws {
        let (vm, _, upload) = try await fixture()
        vm.selectVideoInputImage(from: upload)
        try await wait { !vm.isLoadingVideoInputImage }
        let previous = vm.videoInputImage
        let asset = vm.history[0].images[1]
        try FileManager.default.removeItem(at: XCTUnwrap(asset.url))
        vm.useGeneratedImageForVideo(asset)
        try await wait { !vm.isLoadingVideoInputImage }
        XCTAssertNotNil(vm.errorMessage)
        XCTAssertEqual(vm.videoInputImage, previous)
        XCTAssertFalse(vm.isGeneratingVideo)
    }

    func testPreviewLoaderReadsBase64AndSavedFilesAndDownloadsWithoutCredentials() async throws {
        let (vm, _, _) = try await fixture()
        let asset = vm.history[0].images[0]
        let local = try await MediaStudioImageLoader.data(for: asset)
        let memory = try await MediaStudioImageLoader.data(for: .init(id: "memory", mimeType: "image/png", base64Data: local.base64EncodedString()))
        XCTAssertEqual(memory, local)
        let transport = ImagePreviewTransport(data: local)
        let remote = try await MediaStudioImageLoader.data(
            for: .init(id: "remote", mimeType: "image/png", url: URL(string: "https://cdn.example/result.png")!),
            transport: transport
        )
        XCTAssertEqual(remote, local)
        let requests = await transport.requests
        XCTAssertEqual(requests.count, 1)
        XCTAssertEqual(requests[0].method, "GET")
        XCTAssertNil(requests[0].headers["Authorization"])
    }

    func testPreviewLoaderRejectsUntrustedSchemeWithoutNetworkRequest() async throws {
        let transport = ImagePreviewTransport(data: Data())
        do {
            _ = try await MediaStudioImageLoader.data(
                for: .init(id: "bad", mimeType: "image/png", url: URL(string: "http://cdn.example/result.png")!),
                transport: transport
            )
            XCTFail("Insecure remote assets must not be requested")
        } catch {
            let requests = await transport.requests
            XCTAssertTrue(requests.isEmpty)
        }
    }

    func testGeneratedRemoteReferenceStreamsIntoManagedFile() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MediaStudioRemoteReferenceTests-\(UUID().uuidString)",
            isDirectory: true
        )
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 64, pixelsHigh: 32,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        bitmap.bitmapData?.initialize(repeating: 90, count: bitmap.bytesPerRow * bitmap.pixelsHigh)
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let transport = StreamingImageTransport(data: png)
        let importer = MediaStudioImageReferenceImporter(
            transport: transport,
            artifacts: .init(root: root)
        )

        let imported = try await importer.importGeneratedAssets([.init(
            id: "remote-reference",
            mimeType: "image/png",
            url: URL(string: "https://cdn.example/reference.png")!
        )])

        XCTAssertEqual(imported.count, 1)
        XCTAssertEqual(imported[0].name, "generated-remote-reference.png")
        XCTAssertTrue(imported[0].base64Data.isEmpty)
        XCTAssertEqual(try Data(contentsOf: XCTUnwrap(imported[0].fileURL)), png)
        let streamRequestCount = await transport.streamRequestCount
        let dataRequestCount = await transport.dataRequestCount
        XCTAssertEqual(streamRequestCount, 1)
        XCTAssertEqual(dataRequestCount, 0)
        let requests = await transport.requests
        XCTAssertEqual(requests.count, 1)
        XCTAssertNil(requests[0].headers["Authorization"])
    }

    func testPreviewLoaderRejectsOversizedBase64BeforeDecode() async {
        let asset = GeneratedMediaAsset(
            id: "oversized-inline",
            mimeType: "image/png",
            base64Data: String(
                repeating: "A",
                count: MediaStudioImageLoader.maximumEncodedCharacters + 1
            )
        )

        do {
            _ = try await MediaStudioImageLoader.data(for: asset)
            XCTFail("Oversized inline image was accepted")
        } catch is MediaStudioImageLoader.ImageError {
            // Expected.
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }

    func testFileBackedNormalizationKeepsOwnerAliveAndProducesBoundedPNG() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MediaStudioFileNormalizationTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 128, pixelsHigh: 64,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        let source = root.appendingPathComponent("response.tmp")
        let sourcePNG = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        try sourcePNG.write(to: source)
        var asset: GeneratedMediaAsset? = .init(
            id: "file-backed-normalization",
            mimeType: "image/png",
            mediaFile: GeneratedMediaFile(
                url: source,
                byteCount: sourcePNG.count,
                deleteWhenReleased: true
            )
        )

        var normalized: GeneratedMediaFile? = try await MediaStudioImageLoader.normalizedPNGFile(
            for: try XCTUnwrap(asset),
            ownerUserID: "test-user",
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: TestMediaWorkerImageNormalizer()
        )
        let outputURL = try XCTUnwrap(normalized?.url)
        let outputArtifact = outputURL.deletingLastPathComponent()
        let representation = try XCTUnwrap(NSBitmapImageRep(data: Data(contentsOf: outputURL)))
        XCTAssertEqual(representation.pixelsWide, 128)
        XCTAssertEqual(representation.pixelsHigh, 64)
        XCTAssertTrue(FileManager.default.fileExists(atPath: source.path))

        asset = nil
        XCTAssertFalse(FileManager.default.fileExists(atPath: source.path))
        normalized = nil
        XCTAssertFalse(FileManager.default.fileExists(atPath: outputArtifact.path))
    }

    func testNormalizedPNGFileWritesValidLeaseOwnedArtifact() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MediaStudioNormalizedPNGFileTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 80, pixelsHigh: 40,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        bitmap.bitmapData?.initialize(repeating: 120, count: bitmap.bytesPerRow * bitmap.pixelsHigh)
        let sourcePNG = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        var normalized: GeneratedMediaFile? = try await MediaStudioImageLoader.normalizedPNGFile(
            for: .init(
                id: "normalized-file",
                mimeType: "image/png",
                base64Data: sourcePNG.base64EncodedString()
            ),
            ownerUserID: "test-user",
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: TestMediaWorkerImageNormalizer()
        )

        let outputURL = try XCTUnwrap(normalized?.url)
        let artifact = outputURL.deletingLastPathComponent()
        let representation = try XCTUnwrap(NSBitmapImageRep(data: Data(contentsOf: outputURL)))
        XCTAssertEqual(representation.pixelsWide, 80)
        XCTAssertEqual(representation.pixelsHigh, 40)
        XCTAssertEqual(normalized?.byteCount, try Data(contentsOf: outputURL).count)
        XCTAssertTrue(artifact.lastPathComponent.hasPrefix(".chatos-image-normalized-"))

        normalized = nil
        XCTAssertFalse(FileManager.default.fileExists(atPath: artifact.path))
    }

    func testRemoteNormalizationStreamsThroughTemporaryFile() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MediaStudioRemoteNormalizationTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 72, pixelsHigh: 36,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        bitmap.bitmapData?.initialize(repeating: 75, count: bitmap.bytesPerRow * bitmap.pixelsHigh)
        let sourcePNG = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let transport = StreamingImageTransport(data: sourcePNG)

        var normalized: GeneratedMediaFile? = try await MediaStudioImageLoader.normalizedPNGFile(
            for: .init(
                id: "remote-normalization",
                mimeType: "image/png",
                url: URL(string: "https://cdn.example/normalize.png")!
            ),
            ownerUserID: "test-user",
            transport: transport,
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: TestMediaWorkerImageNormalizer()
        )

        let outputURL = try XCTUnwrap(normalized?.url)
        let outputArtifact = outputURL.deletingLastPathComponent()
        let representation = try XCTUnwrap(NSBitmapImageRep(data: Data(contentsOf: outputURL)))
        XCTAssertEqual(representation.pixelsWide, 72)
        XCTAssertEqual(representation.pixelsHigh, 36)
        let streamRequestCount = await transport.streamRequestCount
        let dataRequestCount = await transport.dataRequestCount
        XCTAssertEqual(streamRequestCount, 1)
        XCTAssertEqual(dataRequestCount, 0)
        let remainingArtifacts = try FileManager.default.contentsOfDirectory(
            at: root,
            includingPropertiesForKeys: nil
        )
        XCTAssertEqual(
            remainingArtifacts.map(\.lastPathComponent),
            [outputArtifact.lastPathComponent]
        )

        normalized = nil
        XCTAssertFalse(FileManager.default.fileExists(atPath: outputArtifact.path))
    }

    func testRemoteThumbnailStreamsThroughTemporaryFile() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MediaStudioRemoteThumbnailTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 128, pixelsHigh: 64,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        bitmap.bitmapData?.initialize(repeating: 45, count: bitmap.bytesPerRow * bitmap.pixelsHigh)
        let sourcePNG = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let transport = StreamingImageTransport(data: sourcePNG)

        let thumbnail = try await MediaStudioImageLoader.thumbnail(
            for: .init(
                id: "remote-thumbnail",
                mimeType: "image/png",
                url: URL(string: "https://cdn.example/thumbnail.png")!
            ),
            ownerUserID: "test-user",
            maximumDisplayPixelSize: 32,
            transport: transport,
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: TestMediaWorkerImageNormalizer()
        )

        XCTAssertEqual(thumbnail.image.width, 32)
        XCTAssertEqual(thumbnail.image.height, 16)
        let streamRequestCount = await transport.streamRequestCount
        let dataRequestCount = await transport.dataRequestCount
        XCTAssertEqual(streamRequestCount, 1)
        XCTAssertEqual(dataRequestCount, 0)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: root.path).isEmpty)
    }

    func testFileBackedThumbnailCacheKeepsOwnerThroughDirectDecode() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "MediaStudioFileThumbnailTests-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 96, pixelsHigh: 48,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        let source = root.appendingPathComponent("response.tmp")
        let sourcePNG = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        try sourcePNG.write(to: source)
        var asset: GeneratedMediaAsset? = .init(
            id: "file-backed-thumbnail-\(UUID().uuidString)",
            mimeType: "image/png",
            mediaFile: GeneratedMediaFile(
                url: source,
                byteCount: sourcePNG.count,
                deleteWhenReleased: true
            )
        )

        let image = await GeneratedMediaAssetImageCache.image(
            for: try XCTUnwrap(asset),
            ownerUserID: "test-user",
            maximumDisplayPixelSize: 24,
            temporaryDirectory: root,
            availableCapacityOverride: Int64.max,
            mediaWorker: TestMediaWorkerImageNormalizer()
        )
        let representation = try XCTUnwrap(image?.representations.first)
        XCTAssertLessThanOrEqual(representation.pixelsWide, 24)
        XCTAssertLessThanOrEqual(representation.pixelsHigh, 24)
        XCTAssertTrue(FileManager.default.fileExists(atPath: source.path))

        asset = nil
        XCTAssertFalse(FileManager.default.fileExists(atPath: source.path))
    }

    func testNormalizedPNGConversionRejectsInvalidImageData() async {
        do {
            _ = try await MediaStudioImageLoader.normalizedPNGFile(for: .init(
                id: "invalid-image",
                mimeType: "image/png",
                base64Data: Data("not-an-image".utf8).base64EncodedString()
            ), ownerUserID: "test-user", mediaWorker: TestMediaWorkerImageNormalizer())
            XCTFail("Invalid image data was accepted")
        } catch is MediaStudioImageLoader.ImageError {
            // Expected.
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }

    func testSharedGeneratedAssetCacheDownsamplesStoryThumbnail() async throws {
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 128, pixelsHigh: 64,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let image = await GeneratedMediaAssetImageCache.image(
            for: .init(
                id: "story-thumbnail-test",
                mimeType: "image/png",
                base64Data: png.base64EncodedString()
            ),
            ownerUserID: "test-user",
            maximumDisplayPixelSize: 24,
            availableCapacityOverride: Int64.max,
            mediaWorker: TestMediaWorkerImageNormalizer()
        )

        let representation = try XCTUnwrap(image?.representations.first)
        XCTAssertLessThanOrEqual(representation.pixelsWide, 24)
        XCTAssertLessThanOrEqual(representation.pixelsHigh, 24)
    }

    private func fixture() async throws -> (MediaStudioViewModel, ImageReferenceCaptureService, URL) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("MediaStudioInteractionTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 256, pixelsHigh: 256,
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        bitmap.bitmapData?.initialize(repeating: 180, count: bitmap.bytesPerRow * bitmap.pixelsHigh)
        let png = try XCTUnwrap(bitmap.representation(using: .png, properties: [:]))
        let upload = root.appendingPathComponent("upload.png")
        try png.write(to: upload)
        let store = MediaStudioHistoryStore(root: root.appendingPathComponent("history"))
        _ = try await store.saveImage(.init(
            id: "batch", modelConfigID: "image", modelName: "Image Model", createdAt: "2026-09-09T00:00:00Z",
            images: ["first", "second"].map { .init(id: $0, mimeType: "image/png", base64Data: png.base64EncodedString()) }
        ), prompt: "saved images", owner: "test-account")
        let service = ImageReferenceCaptureService()
        let vm = MediaStudioViewModel(
            service: service,
            historyStore: store,
            videoJobStore: MediaStudioVideoJobStore(root: root.appendingPathComponent("jobs"))
        )
        vm.activate(userID: "test-account")
        vm.loadIfNeeded()
        try await wait {
            !vm.isLoadingHistory && !vm.isLoadingModels && !vm.isRecoveringVideoJob
        }
        return (vm, service, upload)
    }

    private func waitForHeavyMediaQueue(
        _ governor: ClientHeavyMediaWorkGovernor,
        count: Int
    ) async throws {
        for _ in 0..<200 {
            if await governor.snapshot().queued.count == count { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        XCTFail("Timed out waiting for heavy-media admission")
    }

    private func wait(_ condition: () -> Bool) async throws {
        for _ in 0..<200 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Timed out waiting for image selection")
    }
}

private struct RejectingVideoFrameWorker: MediaWorkerVideoFrameExtracting {
    func extractLastVideoFramePNG(
        sourceURL: URL,
        destinationURL: URL,
        ownerUserID: String,
        maximumSourceBytes: Int,
        maximumOutputPixelSize: Int,
        maximumOutputBytes: Int
    ) async throws -> MediaWorkerImageDescriptor {
        throw MediaWorkerProcessClientError.requestRejected(code: "invalid_video")
    }
}

struct TestMediaWorkerImageNormalizer: MediaWorkerImageNormalizing {
    func normalizePNG(
        sourceURL: URL,
        sourceIdentity authorizedSourceIdentity: MediaWorkerFileIdentity?,
        destinationURL: URL,
        ownerUserID: String,
        maximumSourceBytes: Int,
        maximumSourcePixelCount: Int,
        maximumOutputPixelSize: Int,
        maximumOutputBytes: Int
    ) async throws -> MediaWorkerImageDescriptor {
        do {
            let sourceIdentity: MediaWorkerFileIdentity
            if let authorizedSourceIdentity {
                sourceIdentity = authorizedSourceIdentity
            } else {
                sourceIdentity = try identity(sourceURL)
            }
            let destinationIdentity = try identity(destinationURL)
            let request = MediaWorkerNormalizePNGRequest(
                protocolVersion: ChatOSMediaWorkerProtocol.maximumVersion,
                requestID: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                accountSessionID: "test-session",
                sessionEpoch: "test-epoch",
                helperInstanceID: "in-process-test-worker",
                deadlineUnixMilliseconds: Int64((Date().timeIntervalSince1970 + 30) * 1_000),
                sourcePath: sourceURL.path,
                sourceIdentity: sourceIdentity,
                destinationPath: destinationURL.path,
                destinationIdentity: destinationIdentity,
                maximumSourceBytes: maximumSourceBytes,
                maximumSourcePixelCount: maximumSourcePixelCount,
                maximumOutputPixelSize: maximumOutputPixelSize,
                maximumOutputBytes: maximumOutputBytes
            )
            return try await Task.detached(priority: .userInitiated) {
                try ChatOSMediaWorkerEngine.normalizePNG(request)
            }.value
        } catch let error as MediaWorkerCoreError {
            throw MediaWorkerProcessClientError.requestRejected(code: error.payload.code)
        }
    }

    private func identity(_ url: URL) throws -> MediaWorkerFileIdentity {
        var status = stat()
        guard url.path.withCString({ Darwin.lstat($0, &status) }) == 0 else {
            throw MediaWorkerProcessClientError.invalidResponse
        }
        return .init(
            device: UInt64(status.st_dev),
            inode: UInt64(status.st_ino),
            byteCount: Int64(status.st_size)
        )
    }
}

private actor ImageReferenceCaptureService: MediaGenerationServicing {
    var videoRequests: [VideoGenerationRequest] = []
    func fetchModels() async throws -> [MediaGenerationModel] {
        [.init(id: "h3", name: "MiniMax H3", provider: "gpt", modelName: "MiniMax-H3", enabled: true, taskEnabled: false, hasAPIKey: true)]
    }
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        throw URLError(.unsupportedURL)
    }
    func generateVideo(_ request: VideoGenerationRequest, progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        videoRequests.append(request)
        throw URLError(.unsupportedURL)
    }
}

private actor ImagePreviewTransport: HTTPTransport {
    let data: Data
    var requests: [HTTPRequest] = []
    init(data: Data) { self.data = data }
    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(request)
        return .init(statusCode: 200, headers: ["content-type": "image/png"], body: data)
    }
}

private actor StreamingImageTransport: HTTPTransport {
    let data: Data
    var requests: [HTTPRequest] = []
    var streamRequestCount = 0
    var dataRequestCount = 0

    init(data: Data) { self.data = data }

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        dataRequestCount += 1
        requests.append(request)
        return .init(statusCode: 200, headers: [:], body: data)
    }

    func stream(_ request: HTTPRequest) async throws -> HTTPStreamResponse {
        streamRequestCount += 1
        requests.append(request)
        let midpoint = data.count / 2
        return .init(
            statusCode: 200,
            headers: ["content-length": String(data.count)],
            body: AsyncThrowingStream { continuation in
                continuation.yield(Data(data.prefix(midpoint)))
                continuation.yield(Data(data.suffix(from: midpoint)))
                continuation.finish()
            }
        )
    }
}
