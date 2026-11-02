import AppKit
import ChatOSCore
import SwiftUI
import XCTest
@testable import ChatOSApp

@MainActor
final class TurnProcessLayoutTests: XCTestCase {
    func testLongProcessFitsChatViewportAndResizesWithoutOpeningAnotherWindow() async throws {
        let turn = ConversationTurn(
            id: "layout-turn", sessionID: "layout-session", sequence: 1, revision: 1,
            userMessage: ChatMessage(id: "layout-message", role: .user, text: "检查任务过程",
                                     createdAt: Date(timeIntervalSince1970: 1)),
            status: .completed, startedAt: Date(timeIntervalSince1970: 1)
        )
        let viewModel = TurnProcessViewModel(turn: turn, service: LongProcessLayoutService())
        let measurement = ProcessLayoutMeasurement()
        let content = TurnProcessSheet(viewModel: viewModel, onClose: {})
            .onGeometryChange(for: CGSize.self, of: { $0.size }) { measurement.size = $0 }
        let hosting = NSHostingController(rootView: content)
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 600),
                              styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentViewController = hosting
        window.setContentSize(NSSize(width: 900, height: 600))
        defer {
            viewModel.stop()
            window.close()
        }

        for _ in 0..<100 where viewModel.nodes.isEmpty {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(viewModel.nodes.count, 100)
        try await settle(hosting.view)
        XCTAssertEqual(try XCTUnwrap(measurement.size).height, 600, accuracy: 1)
        XCTAssertEqual(try XCTUnwrap(measurement.size).width, 900, accuracy: 1)
        XCTAssertTrue(window.sheets.isEmpty)

        window.setContentSize(NSSize(width: 500, height: 350))
        try await settle(hosting.view)
        XCTAssertEqual(try XCTUnwrap(measurement.size).height, 350, accuracy: 1)
        XCTAssertEqual(try XCTUnwrap(measurement.size).width, 500, accuracy: 1)
        XCTAssertLessThanOrEqual(hosting.view.fittingSize.height, 350)
        XCTAssertLessThanOrEqual(hosting.view.fittingSize.width, 500)
        XCTAssertTrue(window.sheets.isEmpty)
    }

    private func settle(_ view: NSView) async throws {
        for _ in 0..<5 {
            view.layoutSubtreeIfNeeded()
            try await Task.sleep(for: .milliseconds(20))
        }
    }
}

@MainActor
private final class ProcessLayoutMeasurement {
    var size: CGSize?
}

private struct LongProcessLayoutService: TurnProcessServicing {
    func fetchProcessNodes(sessionID: String, turnID: String) async throws -> [TurnProcessNode] {
        (0..<100).map {
            TurnProcessNode(
                id: "node-\($0)", title: "步骤 \($0)",
                detail: String(repeating: "任务执行详情，需要在视口内滚动查看。", count: 20),
                status: .completed, kind: .tool, timestamp: nil
            )
        }
    }
}
