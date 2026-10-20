import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class TurnProcessViewModelTests: XCTestCase {
    func testLoadPublishesUseCaseResult() async throws {
        let service = TurnProcessServiceStub()
        let viewModel = TurnProcessViewModel(turn: makeTurn(), service: service)

        viewModel.load()
        try await waitUntilLoaded(viewModel)

        XCTAssertEqual(viewModel.nodes.map(\.id), ["node-1"])
        XCTAssertNil(viewModel.errorMessage)
        let requests = await service.recordedRequests()
        XCTAssertEqual(requests, ["session-1:turn-1"])
    }

    func testStopRejectsLateResult() async throws {
        let service = TurnProcessServiceStub(delay: .milliseconds(100))
        let viewModel = TurnProcessViewModel(turn: makeTurn(), service: service)

        viewModel.load()
        XCTAssertTrue(viewModel.isLoading)
        viewModel.stop()
        try await Task.sleep(for: .milliseconds(130))

        XCTAssertFalse(viewModel.isLoading)
        XCTAssertTrue(viewModel.nodes.isEmpty)
        XCTAssertNil(viewModel.errorMessage)
    }

    func testInFlightLoadDoesNotRetainViewModel() async throws {
        let service = TurnProcessServiceStub(delay: .milliseconds(100))
        var viewModel: TurnProcessViewModel? = TurnProcessViewModel(
            turn: makeTurn(),
            service: service
        )
        weak let weakViewModel = viewModel

        viewModel?.load()
        viewModel = nil
        try await Task.sleep(for: .milliseconds(20))

        XCTAssertNil(weakViewModel)
    }

    private func waitUntilLoaded(
        _ viewModel: TurnProcessViewModel
    ) async throws {
        for _ in 0..<100 where viewModel.isLoading {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertFalse(viewModel.isLoading)
    }

    private func makeTurn() -> ConversationTurn {
        ConversationTurn(
            id: "turn-1",
            sessionID: "session-1",
            sequence: 1,
            revision: 1,
            userMessage: ChatMessage(
                id: "message-1",
                role: .user,
                text: "执行任务",
                createdAt: Date(timeIntervalSince1970: 1)
            ),
            status: .completed,
            startedAt: Date(timeIntervalSince1970: 1)
        )
    }
}

private actor TurnProcessServiceStub: TurnProcessServicing {
    private let delay: Duration?
    private var requests: [String] = []

    init(delay: Duration? = nil) {
        self.delay = delay
    }

    func fetchProcessNodes(
        sessionID: String,
        turnID: String
    ) async throws -> [TurnProcessNode] {
        requests.append("\(sessionID):\(turnID)")
        if let delay {
            try? await Task.sleep(for: delay)
        }
        return [TurnProcessNode(
            id: "node-1",
            title: "Tool",
            detail: nil,
            status: .completed,
            kind: .tool,
            timestamp: Date(timeIntervalSince1970: 1)
        )]
    }

    func recordedRequests() -> [String] {
        requests
    }
}
