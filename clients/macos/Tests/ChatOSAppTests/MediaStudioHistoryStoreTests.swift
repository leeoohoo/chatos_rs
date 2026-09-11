import AppKit
import ChatOSAPI
import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

final class MediaStudioHistoryStoreTests: XCTestCase {
    private func directory() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("MediaStudioTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return url
    }

    func testImageAndVideoSurviveNewStoreAndAreIsolatedByAccount() async throws {
        let root = try directory()
        let first = MediaStudioHistoryStore(root: root)
        let image = try await first.saveImage(Self.image(), prompt: "original prompt", owner: "alice")
        let video = try await first.saveVideo(Self.video(), prompt: "video prompt", owner: "alice")
        XCTAssertNil(image.images[0].base64Data)
        XCTAssertEqual(try Data(contentsOf: XCTUnwrap(image.images[0].url)), Self.png)
        XCTAssertEqual(try Data(contentsOf: video.fileURL), Self.video().videoData)
        let second = MediaStudioHistoryStore(root: root)
        let restored = try await second.load(owner: "alice")
        XCTAssertEqual(restored.images, [image])
        XCTAssertEqual(restored.videos, [video])
        XCTAssertEqual(restored.images[0].prompt, "original prompt")
        XCTAssertEqual(restored.images[0].images[0].revisedPrompt, "revised")
        let other = try await second.load(owner: "bob")
        XCTAssertTrue(other.images.isEmpty)
        XCTAssertTrue(other.videos.isEmpty)
    }

    func testURLImagesAreDownloadedOnceAndRestoreOffline() async throws {
        let root = try directory()
        let transport = HistoryImageTransport()
        var result = Self.image()
        result.images = [.init(id: "remote", mimeType: "image/png", url: URL(string: "https://cdn.example/temporary.png")!)]
        let first = MediaStudioHistoryStore(root: root, transport: transport)
        let item = try await first.saveImage(result, prompt: "remote prompt", owner: "alice")
        let restored = try await MediaStudioHistoryStore(root: root, transport: transport).load(owner: "alice")
        XCTAssertEqual(restored.images, [item])
        XCTAssertEqual(try Data(contentsOf: XCTUnwrap(restored.images[0].images[0].url)), Self.png)
        let requests = await transport.requests
        XCTAssertEqual(requests.count, 1)
        XCTAssertNil(requests[0].headers["Authorization"])
    }

    func testCorruptRecordDoesNotHideOtherCreationsOrGetOverwritten() async throws {
        let root = try directory()
        let store = MediaStudioHistoryStore(root: root)
        let good = try await store.saveImage(Self.image(), prompt: "good", owner: "alice")
        let bad = try await store.saveImage(Self.image(), prompt: "bad", owner: "alice")
        let manifest = try XCTUnwrap(bad.images[0].url).deletingLastPathComponent().appendingPathComponent("record.json")
        let brokenData = Data("broken JSON".utf8)
        try brokenData.write(to: manifest)
        let loaded = try await store.load(owner: "alice")
        XCTAssertEqual(loaded.images, [good])
        XCTAssertEqual(loaded.unreadableCount, 1)
        XCTAssertEqual(try Data(contentsOf: manifest), brokenData)
        _ = try await store.saveImage(Self.image(), prompt: "another", owner: "alice")
        XCTAssertEqual(try Data(contentsOf: manifest), brokenData)
    }

    func testRecordsUseRelativeFilenamesAndCanMoveWithAppData() async throws {
        let parent = try directory()
        let root = parent.appendingPathComponent("original")
        _ = try await MediaStudioHistoryStore(root: root).saveImage(Self.image(), prompt: "move", owner: "alice")
        let moved = parent.appendingPathComponent("moved")
        try FileManager.default.moveItem(at: root, to: moved)
        let restored = try await MediaStudioHistoryStore(root: moved).load(owner: "alice")
        XCTAssertEqual(restored.images.count, 1)
        let url = try XCTUnwrap(restored.images[0].images[0].url)
        XCTAssertTrue(url.path.hasPrefix(moved.resolvingSymlinksInPath().path))
        XCTAssertEqual(try Data(contentsOf: url), Self.png)
    }

    @MainActor
    func testGeneratedImageIsRestoredByNewViewModelAndSignOutKeepsDiskHistory() async throws {
        let root = try directory()
        let vm = MediaStudioViewModel(service: HistoryGenerationService(), historyStore: .init(root: root))
        vm.activate(userID: "alice")
        vm.loadIfNeeded()
        try await wait { !vm.isLoadingHistory && !vm.isLoadingModels }
        vm.prompt = "persist this"
        vm.generate()
        try await wait { !vm.isGenerating }
        XCTAssertEqual(vm.history.count, 1)
        XCTAssertNil(vm.historyErrorMessage)
        let saved = vm.history
        vm.resetForSignedOut()
        XCTAssertTrue(vm.history.isEmpty)
        let restarted = MediaStudioViewModel(service: HistoryGenerationService(), historyStore: .init(root: root))
        restarted.activate(userID: "alice")
        try await wait { !restarted.isLoadingHistory }
        XCTAssertEqual(restarted.history, saved)
        restarted.activate(userID: "bob")
        try await wait { !restarted.isLoadingHistory }
        XCTAssertTrue(restarted.history.isEmpty)
        restarted.activate(userID: "alice")
        try await wait { !restarted.isLoadingHistory }
        XCTAssertEqual(restarted.history, saved)
    }

    @MainActor
    func testResultArrivingAfterAccountSwitchCannotAppearInOtherAccount() async throws {
        let root = try directory()
        let service = DelayedHistoryGenerationService()
        let vm = MediaStudioViewModel(service: service, historyStore: .init(root: root))
        vm.activate(userID: "alice")
        vm.loadIfNeeded()
        try await wait { !vm.isLoadingHistory && !vm.isLoadingModels }
        vm.prompt = "alice only"
        vm.generate()
        for _ in 0..<100 {
            if await service.started { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        vm.activate(userID: "bob")
        await service.finish()
        try await wait { !vm.isLoadingHistory }
        try await Task.sleep(for: .milliseconds(30))
        XCTAssertTrue(vm.history.isEmpty)
        let bob = try await MediaStudioHistoryStore(root: root).load(owner: "bob")
        XCTAssertTrue(bob.images.isEmpty)
    }

    @MainActor
    func testMultipleGeneratedHistoryImagesCanBeAddedAsOrderedReferences() async throws {
        let root = try directory()
        var result = Self.image()
        result.images = [
            .init(id: "first", mimeType: "image/png", base64Data: Self.png.base64EncodedString()),
            .init(id: "second", mimeType: "image/png", base64Data: Self.png.base64EncodedString()),
        ]
        _ = try await MediaStudioHistoryStore(root: root).saveImage(result, prompt: "two references", owner: "alice")
        let vm = MediaStudioViewModel(service: HistoryGenerationService(), historyStore: .init(root: root))
        vm.activate(userID: "alice")
        try await wait { !vm.isLoadingHistory }
        let assets = try XCTUnwrap(vm.history.first?.images)
        vm.addGeneratedImagesAsReferences(assets)
        try await wait { !vm.isLoadingInputImages }
        XCTAssertNil(vm.errorMessage, vm.errorMessage ?? "")
        XCTAssertEqual(vm.inputImages.count, 2)
        XCTAssertEqual(vm.inputImages.map(\.name), ["generated-first.png", "generated-second.png"])
    }

    @MainActor
    private func wait(_ condition: () -> Bool) async throws {
        for _ in 0..<200 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Timed out waiting for media studio")
    }

    static let png: Data = {
        let bitmap = NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: 4,
            pixelsHigh: 4,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        )!
        return bitmap.representation(using: .png, properties: [:])!
    }()

    static func image() -> ImageGenerationResult {
        .init(id: "provider-image", modelConfigID: "image", modelName: "gpt-image-2", createdAt: "2026-09-09T00:00:00Z",
              images: [.init(id: "asset", mimeType: "image/png", base64Data: png.base64EncodedString(), revisedPrompt: "revised")])
    }

    static func video() -> VideoGenerationResult {
        .init(id: "provider-video", modelConfigID: "video", modelName: "MiniMax-H3", createdAt: "2026-09-09T00:00:00Z",
              mimeType: "video/mp4", videoData: Data("fixture video".utf8))
    }
}

private actor HistoryImageTransport: HTTPTransport {
    var requests: [HTTPRequest] = []
    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(request)
        return .init(statusCode: 200, headers: ["content-type": "image/png"], body: MediaStudioHistoryStoreTests.png)
    }
}

private struct HistoryGenerationService: MediaGenerationServicing {
    func fetchModels() async throws -> [MediaGenerationModel] {
        [.init(id: "image", name: "Image", provider: "gpt", modelName: "gpt-image-2", enabled: true, taskEnabled: false, hasAPIKey: true)]
    }
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        MediaStudioHistoryStoreTests.image()
    }
    func generateVideo(_ request: VideoGenerationRequest, progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        MediaStudioHistoryStoreTests.video()
    }
}

private actor DelayedHistoryGenerationService: MediaGenerationServicing {
    var started = false
    private var continuation: CheckedContinuation<ImageGenerationResult, Never>?
    func fetchModels() async throws -> [MediaGenerationModel] { try await HistoryGenerationService().fetchModels() }
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        started = true
        return await withCheckedContinuation { continuation = $0 }
    }
    func finish() { continuation?.resume(returning: MediaStudioHistoryStoreTests.image()); continuation = nil }
    func generateVideo(_ request: VideoGenerationRequest, progress: @escaping @Sendable (VideoGenerationProgress) async -> Void) async throws -> VideoGenerationResult {
        MediaStudioHistoryStoreTests.video()
    }
}
