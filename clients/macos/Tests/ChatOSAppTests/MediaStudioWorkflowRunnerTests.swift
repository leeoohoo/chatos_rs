import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

final class MediaStudioWorkflowRunnerTests: XCTestCase {
    func testTextNodesRunInOrderAndReceivePreviousOutput() async throws {
        let text = WorkflowTextGenerator(outputs: [.success("first output"), .success("second output")])
        let runner = try makeRunner(text: text)
        let workflow = MediaStudioWorkflow(
            name: "Copy",
            inputPrompt: "legacy input must not be used",
            nodes: [
                .init(kind: .text, modelConfigID: "text", prompt: "first instruction"),
                .init(kind: .text, modelConfigID: "text", prompt: "second instruction"),
            ]
        )
        let events = WorkflowEventRecorder()

        let result = try await runner.run(
            workflow,
            owner: "alice",
            sessionEpoch: UUID()
        ) { await events.record($0) }

        XCTAssertEqual(result.text, "second output")
        let requests = await text.requests
        XCTAssertEqual(requests.count, 2)
        XCTAssertFalse(requests[0].prompt.contains("legacy input must not be used"))
        XCTAssertTrue(requests[0].prompt.contains("first instruction"))
        XCTAssertTrue(requests[1].prompt.contains("first output"))
        let recordedEvents = await events.values()
        XCTAssertEqual(recordedEvents, [
            .started(workflow.nodes[0].id),
            .completed(workflow.nodes[0].id, .text("first output")),
            .started(workflow.nodes[1].id),
            .completed(workflow.nodes[1].id, .text("second output")),
        ])
    }

    func testFailureStopsBeforeLaterNodes() async throws {
        let text = WorkflowTextGenerator(outputs: [
            .success("first output"),
            .failure(WorkflowTestError.failed),
            .success("must not run"),
        ])
        let runner = try makeRunner(text: text)
        let workflow = MediaStudioWorkflow(
            name: "Stop",
            nodes: [1, 2, 3].map {
                .init(kind: .text, modelConfigID: "text", prompt: "node \($0)")
            }
        )

        do {
            _ = try await runner.run(
                workflow,
                owner: "alice",
                sessionEpoch: UUID()
            ) { _ in }
            XCTFail("The workflow should stop at the failed node")
        } catch WorkflowTestError.failed {}
        let requestCount = await text.requestCount
        XCTAssertEqual(requestCount, 2)
    }

    func testImageNodePublishesItsSavedOutput() async throws {
        let result = ImageGenerationResult(
            id: "image-result",
            modelConfigID: "image",
            modelName: "Image Model",
            createdAt: "2026-11-16T00:00:00Z",
            images: [.init(
                id: "asset-1",
                mimeType: "image/png",
                base64Data: Self.png.base64EncodedString()
            )]
        )
        let runner = try makeRunner(
            text: WorkflowTextGenerator(outputs: []),
            media: WorkflowMediaService(imageResult: result)
        )
        let workflow = MediaStudioWorkflow(
            name: "Image",
            nodes: [.init(kind: .image, modelConfigID: "image", prompt: "Render it")]
        )
        let events = WorkflowEventRecorder()

        _ = try await runner.run(
            workflow,
            owner: "alice",
            sessionEpoch: UUID()
        ) { await events.record($0) }

        let recorded = await events.values()
        guard case let .completed(nodeID, .images(item)) = recorded.last else {
            return XCTFail("Expected the image node output event")
        }
        XCTAssertEqual(nodeID, workflow.nodes[0].id)
        XCTAssertEqual(item.images.map(\.id), ["asset-1"])
        XCTAssertTrue(try XCTUnwrap(item.images[0].url).isFileURL)
    }

    private func makeRunner(
        text: WorkflowTextGenerator,
        media: WorkflowMediaService = WorkflowMediaService()
    ) throws -> MediaStudioWorkflowRunner {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("MediaStudioWorkflowRunnerTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let history = MediaStudioHistoryStore(root: root.appendingPathComponent("history"))
        return MediaStudioWorkflowRunner(
            textGenerator: text,
            imageGenerator: .init(generator: media, history: history),
            videoGenerator: .init(
                generator: media,
                history: history,
                store: .init(root: root.appendingPathComponent("jobs"))
            ),
            store: .init(root: root.appendingPathComponent("workflows"))
        )
    }

    private static let png = Data(base64Encoded:
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
    )!
}

private actor WorkflowEventRecorder {
    private var events: [MediaStudioWorkflowRunEvent] = []

    func record(_ event: MediaStudioWorkflowRunEvent) {
        events.append(event)
    }

    func values() -> [MediaStudioWorkflowRunEvent] {
        events
    }
}

private enum WorkflowTestError: Error {
    case failed
}

private actor WorkflowTextGenerator: MediaStudioTextGenerating {
    private var outputs: [Result<String, Error>]
    private(set) var requests: [MediaStudioTextGenerationRequest] = []
    var requestCount: Int { requests.count }

    init(outputs: [Result<String, Error>]) {
        self.outputs = outputs
    }

    func generateText(_ request: MediaStudioTextGenerationRequest) async throws -> String {
        requests.append(request)
        return try outputs.removeFirst().get()
    }
}

private struct WorkflowMediaService: MediaGenerationServicing {
    var imageResult: ImageGenerationResult? = nil

    func fetchModels() async throws -> [MediaGenerationModel] { [] }
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        guard let imageResult else { throw WorkflowTestError.failed }
        return imageResult
    }
    func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        throw WorkflowTestError.failed
    }
}
