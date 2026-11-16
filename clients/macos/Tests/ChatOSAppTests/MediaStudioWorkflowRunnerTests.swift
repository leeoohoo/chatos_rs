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

        let result = try await runner.run(
            workflow,
            input: .init(prompt: "seed", images: []),
            owner: "alice",
            sessionEpoch: UUID()
        ) { _ in }

        XCTAssertEqual(result.text, "second output")
        let requests = await text.requests
        XCTAssertEqual(requests.count, 2)
        XCTAssertTrue(requests[0].prompt.contains("seed"))
        XCTAssertTrue(requests[1].prompt.contains("first output"))
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
                input: .init(prompt: "", images: []),
                owner: "alice",
                sessionEpoch: UUID()
            ) { _ in }
            XCTFail("The workflow should stop at the failed node")
        } catch WorkflowTestError.failed {}
        let requestCount = await text.requestCount
        XCTAssertEqual(requestCount, 2)
    }

    private func makeRunner(text: WorkflowTextGenerator) throws -> MediaStudioWorkflowRunner {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("MediaStudioWorkflowRunnerTests-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: root) }
        let media = WorkflowMediaService()
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
    func fetchModels() async throws -> [MediaGenerationModel] { [] }
    func generateImage(_ request: ImageGenerationRequest) async throws -> ImageGenerationResult {
        throw WorkflowTestError.failed
    }
    func generateVideo(
        _ request: VideoGenerationRequest,
        progress: @escaping @Sendable (VideoGenerationProgress) async -> Void
    ) async throws -> VideoGenerationResult {
        throw WorkflowTestError.failed
    }
}
