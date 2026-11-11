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
        XCTAssertNil(findScrollView(in: hosting))

        window.setContentSize(NSSize(width: 500, height: 350))
        try await settle(hosting)
        let resized = try XCTUnwrap(measurement.frame)
        XCTAssertEqual(resized.midX, 250, accuracy: 1)
        XCTAssertEqual(resized.midY, 175, accuracy: 1)
        XCTAssertEqual(resized.size, initial.size)
    }

    func testFirstMessageReplacesEmptyStateWithScrollableTimeline() async throws {
        let measurement = EmptyStateMeasurement()
        let hosting = NSHostingView(rootView: timeline(measurement: measurement))
        let window = mount(hosting, width: 900, height: 600)
        defer { window.close() }
        try await settle(hosting)
        hosting.rootView = timeline(items: [.init(id: "first")], measurement: measurement)
        try await settle(hosting)
        XCTAssertNotNil(findScrollView(in: hosting))

        hosting.rootView = timeline(measurement: measurement)
        try await settle(hosting)
        XCTAssertNil(findScrollView(in: hosting))
    }

    func testOlderHistoryActionIsNotCoveredByEmptyPlaceholder() async throws {
        let hosting = NSHostingView(rootView: timeline(hasOlder: true, measurement: .init()))
        let window = mount(hosting, width: 900, height: 600)
        defer { window.close() }
        try await settle(hosting)
        XCTAssertNotNil(findScrollView(in: hosting))
    }

    func testRowsUseNaturalHeightsAndKeepExplicitSpacing() async throws {
        let measurement = EmptyStateMeasurement()
        let hosting = NSHostingView(rootView: timeline(
            items: [
                .init(id: "short", height: 80),
                .init(id: "long", height: 220),
            ],
            measurement: measurement
        ))
        let window = mount(hosting, width: 900, height: 600)
        defer { window.close() }
        try await settle(hosting)

        let short = try XCTUnwrap(measurement.rowFrames["short"])
        let long = try XCTUnwrap(measurement.rowFrames["long"])
        XCTAssertEqual(short.height, 80, accuracy: 1)
        XCTAssertEqual(long.height, 220, accuracy: 1)
        XCTAssertLessThan(short.minY, 40, "a short conversation should stay top-aligned")
        XCTAssertGreaterThanOrEqual(long.minY - short.maxY, 15)
    }

    func testLongTimelinePositionsLatestMessageAtBottom() async throws {
        let measurement = EmptyStateMeasurement()
        let items = (0..<6).map { EmptyStateItem(id: "row-\($0)", height: 180) }
        let hosting = NSHostingView(rootView: timeline(items: items, measurement: measurement))
        let window = mount(hosting, width: 900, height: 600)
        defer { window.close() }
        try await settle(hosting)

        let last = try XCTUnwrap(measurement.rowFrames["row-5"])
        XCTAssertGreaterThan(last.maxY, 500)
        XCTAssertLessThanOrEqual(last.maxY, 600)
    }

    func testLargeTimelineConstructsOnlyVisibleRowsWhileScrolling() async throws {
        let measurement = EmptyStateMeasurement()
        let items = (0..<1_000).map { EmptyStateItem(id: "row-\($0)", height: 100) }
        let hosting = NSHostingView(rootView: timeline(items: items, measurement: measurement))
        let window = mount(hosting, width: 900, height: 600)
        defer { window.close() }
        try await settle(hosting)

        let initialCount = measurement.constructedIDs.count
        XCTAssertGreaterThan(initialCount, 0)
        XCTAssertLessThan(initialCount, 40)

        let scrollView = try XCTUnwrap(findScrollView(in: hosting))
        scrollView.contentView.scroll(to: NSPoint(x: 0, y: 8_000))
        scrollView.reflectScrolledClipView(scrollView.contentView)
        try await settle(hosting)
        XCTAssertGreaterThan(measurement.constructedIDs.count, initialCount)
        XCTAssertLessThan(measurement.constructedIDs.count, 80)
    }

    private func timeline(
        items: [EmptyStateItem] = [], hasOlder: Bool = false, measurement: EmptyStateMeasurement
    ) -> some View {
        AgentChatTimelineView(
            items: items, isInitialContentReady: true, hasOlderItems: hasOlder,
            isLoadingOlderItems: false, scrollToLatestRequest: 0, loadOlderItems: { nil },
            rowContent: { item in
                Text(item.id)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .frame(height: item.height)
                    .onAppear { measurement.constructedIDs.insert(item.id) }
                    .onGeometryChange(
                        for: CGRect.self,
                        of: { $0.frame(in: .named("viewport")) }
                    ) { measurement.rowFrames[item.id] = $0 }
            },
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

    private func findScrollView(in view: NSView) -> NSScrollView? {
        if let scrollView = view as? NSScrollView { return scrollView }
        return view.subviews.lazy.compactMap { self.findScrollView(in: $0) }.first
    }
}

private struct EmptyStateItem: Identifiable, Equatable {
    let id: String
    var height: CGFloat = 20
}

@MainActor
private final class EmptyStateMeasurement {
    var frame: CGRect?
    var rowFrames: [String: CGRect] = [:]
    var constructedIDs: Set<String> = []
}
