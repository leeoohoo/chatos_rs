import AppKit
import SwiftUI
import XCTest
@testable import ChatOSApp

@MainActor
final class AgentChatEmptyStateTests: XCTestCase {
    func testEmptyStateIsCenteredInViewportAndTracksResize() async throws {
        let measurement = EmptyStateMeasurement()
        let hosting = NSHostingView(rootView: timeline(measurement: measurement))
        let window = mount(hosting, width: 900, height: 600)
        defer { window.close() }
        try await settle(hosting)
        let initial = try XCTUnwrap(measurement.frame)
        XCTAssertEqual(initial.midX, 450, accuracy: 1)
        XCTAssertEqual(initial.midY, 300, accuracy: 1)
        XCTAssertNil(findTable(in: hosting))

        window.setContentSize(NSSize(width: 500, height: 350))
        try await settle(hosting)
        let resized = try XCTUnwrap(measurement.frame)
        XCTAssertEqual(resized.midX, 250, accuracy: 1)
        XCTAssertEqual(resized.midY, 175, accuracy: 1)
        XCTAssertEqual(resized.size, initial.size)
    }

    func testFirstMessageReplacesEmptyStateWithNativeTimeline() async throws {
        let measurement = EmptyStateMeasurement()
        let hosting = NSHostingView(rootView: timeline(measurement: measurement))
        let window = mount(hosting, width: 900, height: 600)
        defer { window.close() }
        try await settle(hosting)
        hosting.rootView = timeline(items: [.init(id: "first")], measurement: measurement)
        try await settle(hosting)
        XCTAssertEqual(try XCTUnwrap(findTable(in: hosting)).numberOfRows, 1)

        hosting.rootView = timeline(measurement: measurement)
        try await settle(hosting)
        XCTAssertNil(findTable(in: hosting))
    }

    func testOlderHistoryActionIsNotCoveredByEmptyPlaceholder() async throws {
        let hosting = NSHostingView(rootView: timeline(hasOlder: true, measurement: .init()))
        let window = mount(hosting, width: 900, height: 600)
        defer { window.close() }
        try await settle(hosting)
        // Only the load-older control is a row; no cropped empty state is hosted in the table.
        XCTAssertEqual(try XCTUnwrap(findTable(in: hosting)).numberOfRows, 1)
    }

    private func timeline(
        items: [EmptyStateItem] = [], hasOlder: Bool = false, measurement: EmptyStateMeasurement
    ) -> some View {
        AgentChatTimelineView(
            items: items, isInitialContentReady: true, hasOlderItems: hasOlder,
            isLoadingOlderItems: false, scrollToLatestRequest: 0, loadOlderItems: { nil },
            rowContent: { Text($0.id) },
            emptyContent: {
                Text("开始对话")
                    .frame(width: 160, height: 90)
                    .onGeometryChange(for: CGRect.self, of: { $0.frame(in: .named("viewport")) }) {
                        measurement.frame = $0
                    }
            }
        )
        .coordinateSpace(name: "viewport")
    }

    private func mount(_ view: NSView, width: CGFloat, height: CGFloat) -> NSWindow {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: width, height: height),
                              styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = view
        return window
    }

    private func settle(_ view: NSView) async throws {
        for _ in 0..<5 {
            view.layoutSubtreeIfNeeded()
            try await Task.sleep(for: .milliseconds(20))
        }
    }

    private func findTable(in view: NSView) -> NSTableView? {
        if let table = view as? NSTableView { return table }
        return view.subviews.lazy.compactMap { self.findTable(in: $0) }.first
    }
}

private struct EmptyStateItem: Identifiable, Equatable {
    let id: String
}

@MainActor
private final class EmptyStateMeasurement {
    var frame: CGRect?
}
