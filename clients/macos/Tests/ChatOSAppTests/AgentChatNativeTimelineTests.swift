import AppKit
import SwiftUI
import Testing
@testable import ChatOSApp

@MainActor
@Suite(.serialized)
struct AgentChatNativeTimelineTests {
    private func input(
        entries: [AgentChatTimelineEntry], ready: Bool = false, scrollRequest: Int = 0
    ) -> AgentChatNativeTimeline {
        .init(entries: entries, rowState: .init(), fontScale: 1, colorScheme: .light,
              isInitialContentReady: ready, scrollToLatestRequest: scrollRequest)
    }

    private func mount(_ coordinator: AgentChatTimelineCoordinator) -> NSWindow {
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 600),
                              styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        let scroll = coordinator.makeScrollView()
        scroll.frame = NSRect(x: 0, y: 0, width: 900, height: 600)
        window.contentView = scroll
        window.contentView?.layoutSubtreeIfNeeded()
        return window
    }

    private func settle(_ coordinator: AgentChatTimelineCoordinator) async throws {
        for _ in 0..<5 {
            coordinator.scrollView?.layoutSubtreeIfNeeded()
            coordinator.table.layoutSubtreeIfNeeded()
            try await Task.sleep(for: .milliseconds(20))
        }
    }

    @Test
    func thousandRowsRenderOnlyVisibleMessagesAndIgnoreUnchangedRefreshes() async throws {
        let coordinator = AgentChatTimelineCoordinator()
        let window = mount(coordinator)
        defer { coordinator.stop(); window.close() }
        var constructedIDs: [String] = []
        let entries = (0..<1_000).map { index in
            AgentChatTimelineEntry(id: "message-\(index)", value: index) {
                constructedIDs.append("message-\(index)")
                return AnyView(Text("Message \(index)").frame(height: 100))
            }
        }
        coordinator.update(input(entries: entries))
        try await settle(coordinator)
        #expect(coordinator.table.numberOfRows == 1_000)
        #expect(!constructedIDs.isEmpty)
        #expect(constructedIDs.count < 30)
        let initialCount = constructedIDs.count
        for _ in 0..<500 { coordinator.update(input(entries: entries)) }
        try await settle(coordinator)
        #expect(constructedIDs.count == initialCount)
        window.setContentSize(NSSize(width: 500, height: 600))
        try await settle(coordinator)
        #expect(coordinator.table.rect(ofRow: 0).height == 112)

        let scroll = try #require(coordinator.scrollView)
        scroll.contentView.scroll(to: NSPoint(x: 0, y: 8_000))
        scroll.reflectScrolledClipView(scroll.contentView)
        try await settle(coordinator)
        #expect(constructedIDs.count > initialCount)
        #expect(constructedIDs.count < 60)
        print("CHATOS_TIMELINE_BASELINE rows=1000 initial_constructed=\(initialCount) after_500_refreshes=\(initialCount) after_resize_and_scroll=\(constructedIDs.count)")
    }

    @Test
    func markdownRowsMeasureAsynchronouslyAndResizeWithoutLosingHistoryPosition() async throws {
        let coordinator = AgentChatTimelineCoordinator()
        let window = mount(coordinator)
        defer { coordinator.stop(); window.close() }
        var constructedCount = 0
        let paragraph = String(repeating: "聊天历史需要在窗口宽度变化时正确换行，并保持当前阅读位置。", count: 20)
        let entries = (0..<1_000).map { index in
            AgentChatTimelineEntry(id: "markdown-\(index)", value: index) {
                constructedCount += 1
                return AnyView(MarkdownDocumentView(markdown: "## 消息 \(index)\n\n\(paragraph)"))
            }
        }
        coordinator.update(input(entries: entries, ready: true))
        try await settle(coordinator)
        let scroll = try #require(coordinator.scrollView)
        #expect(constructedCount < 30)
        #expect(coordinator.table.rect(ofRow: 999).height > 140)
        #expect(abs(coordinator.table.bounds.height - scroll.contentView.bounds.maxY) < 2)

        scroll.contentView.scroll(to: NSPoint(x: 0, y: 10_000))
        scroll.reflectScrolledClipView(scroll.contentView)
        try await settle(coordinator)
        let topRow = coordinator.table.rows(in: scroll.contentView.bounds).location
        let anchorID = coordinator.entries[topRow].id
        let before = coordinator.table.rect(ofRow: topRow).minY - scroll.contentView.bounds.minY
        let appended = AgentChatTimelineEntry(id: "appended", value: 1) { AnyView(Text("New message")) }
        coordinator.update(input(entries: entries + [appended], ready: true))
        try await settle(coordinator)
        #expect(abs(coordinator.table.rect(ofRow: topRow).minY - scroll.contentView.bounds.minY - before) < 2)

        window.setContentSize(NSSize(width: 500, height: 600))
        try await settle(coordinator)
        let currentRow = coordinator.table.rows(in: scroll.contentView.bounds).location
        #expect(coordinator.entries[currentRow].id == anchorID)
        #expect(constructedCount < 80)
    }

    @Test
    func prependAndHeightChangesPreserveReadingAnchorAndExplicitScrollReachesBottom() async throws {
        let coordinator = AgentChatTimelineCoordinator()
        let window = mount(coordinator)
        defer { coordinator.stop(); window.close() }
        let entries = (0..<100).map { index in
            AgentChatTimelineEntry(id: "message-\(index)", value: index) {
                AnyView(Text("Message \(index)").frame(height: 100))
            }
        }
        coordinator.update(input(entries: entries, ready: true))
        try await settle(coordinator)
        let scroll = try #require(coordinator.scrollView)
        scroll.contentView.scroll(to: NSPoint(x: 0, y: 4_000))
        scroll.reflectScrolledClipView(scroll.contentView)
        try await settle(coordinator)
        let topRow = coordinator.table.rows(in: scroll.contentView.bounds).location
        let anchorID = coordinator.entries[topRow].id
        let before = coordinator.table.rect(ofRow: topRow).minY - scroll.contentView.bounds.minY
        let older = (0..<20).map { index in
            AgentChatTimelineEntry(id: "older-\(index)", value: index) {
                AnyView(Text("Older \(index)").frame(height: 80))
            }
        }
        coordinator.update(input(entries: older + entries, ready: true))
        try await settle(coordinator)
        let newRow = try #require(coordinator.entries.firstIndex { $0.id == anchorID })
        let after = coordinator.table.rect(ofRow: newRow).minY - scroll.contentView.bounds.minY
        #expect(abs(before - after) < 2)
        coordinator.update(input(entries: older + entries, ready: true, scrollRequest: 1))
        try await settle(coordinator)
        #expect(abs(coordinator.table.bounds.height - scroll.contentView.bounds.maxY) < 2)
    }

    @Test
    func heightCacheIsBoundedAndRejectsInvalidMeasurements() {
        var cache = AgentChatTimelineHeightCache()
        let acceptedNaN = cache.store(.nan, for: "nan")
        let acceptedInfinity = cache.store(.infinity, for: "infinity")
        let acceptedNegative = cache.store(-1, for: "negative")
        #expect(!acceptedNaN && !acceptedInfinity && !acceptedNegative)
        for index in 0..<10_000 { _ = cache.store(100, for: "message-\(index)") }
        #expect(cache.count == AgentChatTimelineHeightCache.capacity)
        #expect(cache.height(for: "message-9999") == 100)
        let repeatedHeightChanged = cache.store(100, for: "message-9999")
        #expect(!repeatedHeightChanged)
    }
}
