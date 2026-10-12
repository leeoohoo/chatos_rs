import AppKit
import SwiftUI
import XCTest
@testable import ChatOSApp

@MainActor
final class AgentTeamHeaderTests: XCTestCase {
    func testRunControlsResistLongHighPriorityTitle() async throws {
        let normal = try await measureControls(containerWidth: 1_000)
        let narrow = try await measureControls(containerWidth: 260)
        XCTAssertGreaterThan(normal.width, 120)
        XCTAssertEqual(narrow.width, normal.width, accuracy: 1)
    }

    func testProgressKeepsBothActionWidthsStable() async throws {
        let normal = try await measureControls(containerWidth: 260)
        let pausing = try await measureControls(containerWidth: 260, isPausing: true)
        let stopping = try await measureControls(containerWidth: 260, isStopping: true)
        XCTAssertEqual(pausing.width, normal.width, accuracy: 1)
        XCTAssertEqual(stopping.width, normal.width, accuracy: 1)
    }

    func testEnlargedFontsDoNotCompressActionLabels() async throws {
        let regular = try await measureControls(containerWidth: 320)
        let wide = try await measureControls(containerWidth: 1_000, fontSize: 22)
        let narrow = try await measureControls(containerWidth: 320, fontSize: 22)
        XCTAssertEqual(narrow.width, wide.width, accuracy: 1)
        XCTAssertGreaterThan(narrow.width, regular.width)
    }

    func testNarrowHeaderMovesTabsToSecondRow() async throws {
        let wide = try await measureHeader(width: 1_200)
        let narrow = try await measureHeader(width: 550)
        XCTAssertGreaterThan(narrow.height, wide.height + 10)
        XCTAssertLessThanOrEqual(narrow.width, 550)
    }

    private func measureControls(
        containerWidth: CGFloat, isPausing: Bool = false, isStopping: Bool = false,
        fontSize: CGFloat = 14
    ) async throws -> CGSize {
        let measurement = HeaderMeasurement()
        let root = HStack {
            Text(String(repeating: "很长的团队介绍", count: 60)).lineLimit(1).layoutPriority(1)
            AgentTeamRunControls(
                isRunning: true, hasInterruptedRuns: true,
                isPausing: isPausing, isStopping: isStopping, onPause: {}, onStop: {}
            )
            .onGeometryChange(for: CGSize.self, of: { $0.size }) { measurement.size = $0 }
        }
        .font(.system(size: fontSize))
        .environment(\.interfaceFontScale, fontSize / 14)
        .frame(width: containerWidth)
        try await render(root, width: containerWidth)
        return try XCTUnwrap(measurement.size)
    }

    private func measureHeader(width: CGFloat) async throws -> CGSize {
        let measurement = HeaderMeasurement()
        let root = AgentTeamHeader(
            name: "FinFlow 财务产品团队", goal: String(repeating: "构建可运行的财务业务流程。", count: 30),
            selectedSection: .constant(.chat), isRunning: true, hasInterruptedRuns: true,
            isPausing: false, isStopping: false, onPause: {}, onStop: {}
        )
        .frame(width: width)
        .fixedSize(horizontal: false, vertical: true)
        .onGeometryChange(for: CGSize.self, of: { $0.size }) { measurement.size = $0 }
        try await render(root, width: width)
        return try XCTUnwrap(measurement.size)
    }

    private func render(_ root: some View, width: CGFloat) async throws {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: width, height: 180),
                              styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        let hosting = NSHostingView(rootView: root.frame(maxHeight: .infinity, alignment: .top))
        window.contentView = hosting
        for _ in 0..<3 {
            hosting.layoutSubtreeIfNeeded()
            try await Task.sleep(for: .milliseconds(20))
        }
    }
}

@MainActor
private final class HeaderMeasurement {
    var size: CGSize?
}
