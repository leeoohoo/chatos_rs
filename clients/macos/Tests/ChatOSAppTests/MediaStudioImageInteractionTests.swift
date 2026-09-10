import AppKit
import ChatOSAPI
import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class MediaStudioImageInteractionTests: XCTestCase {
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
        XCTAssertEqual(vm.prompt, "keep image prompt")
        XCTAssertEqual(vm.videoPrompt, "keep video prompt")
        XCTAssertEqual(vm.history[0].images.count, 2)
        let before = await service.videoRequests
        XCTAssertTrue(before.isEmpty)
        vm.generateVideo()
        try await wait { !vm.isGeneratingVideo }
        let requests = await service.videoRequests
        XCTAssertEqual(requests.count, 1)
        XCTAssertEqual(requests.first?.inputImage, input)
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
        let vm = MediaStudioViewModel(service: service, historyStore: store)
        vm.activate(userID: "test-account")
        vm.loadIfNeeded()
        try await wait { !vm.isLoadingHistory && !vm.isLoadingModels }
        return (vm, service, upload)
    }

    private func wait(_ condition: () -> Bool) async throws {
        for _ in 0..<200 {
            if condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Timed out waiting for image selection")
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
