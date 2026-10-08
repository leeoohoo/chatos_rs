import AppKit
import SwiftUI

struct AgentChatNativeTimeline: NSViewRepresentable {
    let entries: [AgentChatTimelineEntry]
    let rowState: AgentChatTimelineRowState
    let fontScale: CGFloat
    let colorScheme: ColorScheme
    let isInitialContentReady: Bool
    let scrollToLatestRequest: Int

    func makeCoordinator() -> AgentChatTimelineCoordinator { .init() }

    func makeNSView(context: Context) -> AgentChatTimelineScrollView {
        context.coordinator.makeScrollView()
    }

    func updateNSView(_ view: AgentChatTimelineScrollView, context: Context) {
        context.coordinator.update(self)
    }

    static func dismantleNSView(_ view: AgentChatTimelineScrollView, coordinator: AgentChatTimelineCoordinator) {
        coordinator.stop()
    }
}

/// AppKit owns scrolling, visible-row recycling and accessibility. SwiftUI only owns each
/// visible bubble. No offset, mouse-wheel event or scheduler tick is published into the rows.
@MainActor
final class AgentChatTimelineCoordinator: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    private(set) var entries: [AgentChatTimelineEntry] = []
    private(set) var table = NSTableView()
    private(set) var scrollView: AgentChatTimelineScrollView?
    private var rowsByID: [String: Int] = [:]
    private var heights = AgentChatTimelineHeightCache()
    private var input: AgentChatNativeTimeline?
    private var pendingHeights: [String: CGFloat] = [:]
    private var heightUpdateScheduled = false
    private var resizeScheduled = false
    private var initialPositionScheduled = false
    private var hasPositionedInitially = false
    private var followsLatest = true
    private var isAdjustingLayout = false
    private var layoutGeneration = 0
    private var columnWidth: CGFloat = 0
    private var boundsObserver: NSObjectProtocol?
    private let cellIdentifier = NSUserInterfaceItemIdentifier("agent-chat-message")

    private struct Anchor {
        let id: String
        let offset: CGFloat
    }

    func makeScrollView() -> AgentChatTimelineScrollView {
        let scroll = AgentChatTimelineScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = false
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        scroll.automaticallyAdjustsContentInsets = false
        table.headerView = nil
        table.backgroundColor = .clear
        table.style = .plain
        table.selectionHighlightStyle = .none
        table.intercellSpacing = NSSize(width: 0, height: 0)
        table.columnAutoresizingStyle = .lastColumnOnlyAutoresizingStyle
        table.rowSizeStyle = .custom
        table.usesAutomaticRowHeights = false
        table.dataSource = self
        table.delegate = self
        let column = NSTableColumn(identifier: cellIdentifier)
        column.minWidth = 1
        table.addTableColumn(column)
        scroll.documentView = table
        scroll.contentView.postsBoundsChangedNotifications = true
        boundsObserver = NotificationCenter.default.addObserver(
            forName: NSView.boundsDidChangeNotification,
            object: scroll.contentView,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.didScroll() }
        }
        scroll.resized = { [weak self] in self?.scheduleResize() }
        scrollView = scroll
        return scroll
    }

    func stop() {
        if let boundsObserver { NotificationCenter.default.removeObserver(boundsObserver) }
        boundsObserver = nil
        scrollView?.resized = nil
        table.delegate = nil
        table.dataSource = nil
        input = nil
        entries = []
        pendingHeights = [:]
        scrollView = nil
    }

    func update(_ next: AgentChatNativeTimeline) {
        guard let scrollView else { return }
        if columnWidth == 0, scrollView.contentSize.width > 1 {
            columnWidth = scrollView.contentSize.width
            table.tableColumns.first?.width = columnWidth
        }
        let anchor = readingAnchor()
        let previous = input
        let structureChanged = entries.map(\.id) != next.entries.map(\.id)
        let rowStateChanged = previous?.rowState != next.rowState
            || previous?.fontScale != next.fontScale || previous?.colorScheme != next.colorScheme
        let lastChanged = entries.last?.id != next.entries.last?.id
        var changedRows = IndexSet()
        for (index, entry) in next.entries.enumerated() {
            if let oldIndex = rowsByID[entry.id], entry.hasSameContent(as: entries[oldIndex]),
               !rowStateChanged { continue }
            pendingHeights.removeValue(forKey: entry.id)
            changedRows.insert(index)
        }
        input = next
        entries = next.entries
        rowsByID = Dictionary(uniqueKeysWithValues: entries.enumerated().map { ($0.element.id, $0.offset) })
        isAdjustingLayout = true
        if structureChanged {
            table.reloadData()
        } else if !changedRows.isEmpty {
            table.reloadData(forRowIndexes: changedRows, columnIndexes: IndexSet(integer: 0))
            table.noteHeightOfRows(withIndexesChanged: changedRows)
        }
        let explicitScroll = previous != nil && previous?.scrollToLatestRequest != next.scrollToLatestRequest
        if explicitScroll || (hasPositionedInitially && followsLatest && lastChanged) {
            followsLatest = true
            scrollToBottom()
        } else if structureChanged || !changedRows.isEmpty {
            restore(anchor)
        }
        isAdjustingLayout = false
        // Position only after there is an actual viewport. Async Markdown height updates then
        // keep the latest edge pinned, rather than jumping back to estimated row positions.
        if next.isInitialContentReady && !hasPositionedInitially && !initialPositionScheduled {
            initialPositionScheduled = true
            DispatchQueue.main.async { [weak self] in
                guard let self else { return }
                initialPositionScheduled = false
                guard self.scrollView != nil, scrollView.contentSize.height > 0 else { return }
                hasPositionedInitially = true
                followsLatest = true
                scrollToBottom()
            }
        }
    }

    func numberOfRows(in tableView: NSTableView) -> Int { entries.count }

    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        heights.height(for: entries[row].id)
    }

    func tableView(_ tableView: NSTableView, shouldSelectRow row: Int) -> Bool { false }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int) -> NSView? {
        guard entries.indices.contains(row), let input else { return nil }
        let cell = tableView.makeView(withIdentifier: cellIdentifier, owner: nil)
            as? AgentChatTimelineCell ?? AgentChatTimelineCell()
        cell.identifier = cellIdentifier
        configure(cell, entry: entries[row], input: input)
        return cell
    }

    private func configure(_ cell: AgentChatTimelineCell, entry: AgentChatTimelineEntry, input: AgentChatNativeTimeline) {
        let width = max(scrollView?.contentSize.width ?? table.bounds.width, 1)
        let generation = layoutGeneration
        cell.configure(entry: entry, width: width, fontScale: input.fontScale, colorScheme: input.colorScheme) {
            [weak self, weak cell] height in
            guard let self, cell?.representedID == entry.id, generation == layoutGeneration,
                  let row = rowsByID[entry.id], entry.hasSameContent(as: entries[row]) else { return }
            queueHeight(height, id: entry.id)
        }
    }

    private func queueHeight(_ height: CGFloat, id: String) {
        guard height.isFinite, height > 0, rowsByID[id] != nil,
              abs(heights.height(for: id) - ceil(height)) > 0.5 else { return }
        pendingHeights[id] = height
        guard !heightUpdateScheduled else { return }
        heightUpdateScheduled = true
        DispatchQueue.main.async { [weak self] in self?.applyPendingHeights() }
    }

    private func applyPendingHeights() {
        heightUpdateScheduled = false
        guard scrollView != nil else { return }
        let anchor = readingAnchor()
        var changed = IndexSet()
        for (id, height) in pendingHeights {
            if let row = rowsByID[id], heights.store(height, for: id) { changed.insert(row) }
        }
        pendingHeights.removeAll(keepingCapacity: true)
        guard !changed.isEmpty else { return }
        isAdjustingLayout = true
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0
            table.noteHeightOfRows(withIndexesChanged: changed)
            table.layoutSubtreeIfNeeded()
        }
        if followsLatest && hasPositionedInitially { scrollToBottom() } else { restore(anchor) }
        isAdjustingLayout = false
    }

    private func scheduleResize() {
        guard !resizeScheduled else { return }
        resizeScheduled = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            resizeScheduled = false
            guard let scrollView, let input else { return }
            let width = scrollView.contentSize.width
            guard width > 1 else { return }
            if abs(width - columnWidth) > 0.5 {
                let anchor = readingAnchor()
                columnWidth = width
                layoutGeneration += 1
                pendingHeights.removeAll(keepingCapacity: true)
                heights.removeAll()
                table.tableColumns.first?.width = width
                isAdjustingLayout = true
                table.noteHeightOfRows(withIndexesChanged: IndexSet(integersIn: 0..<entries.count))
                let visible = table.rows(in: table.visibleRect)
                if visible.location != NSNotFound {
                    for row in visible.location..<min(NSMaxRange(visible), entries.count) {
                        if let cell = table.view(atColumn: 0, row: row, makeIfNecessary: false) as? AgentChatTimelineCell {
                            configure(cell, entry: entries[row], input: input)
                        }
                    }
                }
                if followsLatest { scrollToBottom() } else { restore(anchor) }
                isAdjustingLayout = false
            }
            if input.isInitialContentReady && !hasPositionedInitially { update(input) }
        }
    }

    private func didScroll() {
        guard !isAdjustingLayout, hasPositionedInitially, let scrollView else { return }
        followsLatest = AgentChatTimelineScrollMetrics.isAtBottom(
            markerMaxY: table.bounds.height - scrollView.contentView.bounds.minY,
            viewportHeight: scrollView.contentSize.height
        )
    }

    private func readingAnchor() -> Anchor? {
        guard let scrollView else { return nil }
        let visible = table.rows(in: scrollView.contentView.bounds)
        guard visible.location != NSNotFound, entries.indices.contains(visible.location) else { return nil }
        return Anchor(id: entries[visible.location].id,
                      offset: scrollView.contentView.bounds.minY - table.rect(ofRow: visible.location).minY)
    }

    private func restore(_ anchor: Anchor?) {
        guard let anchor, let row = rowsByID[anchor.id] else { return }
        scroll(to: table.rect(ofRow: row).minY + anchor.offset)
    }

    private func scrollToBottom() {
        table.layoutSubtreeIfNeeded()
        scroll(to: table.bounds.height - (scrollView?.contentSize.height ?? 0))
    }

    private func scroll(to offset: CGFloat) {
        guard let scrollView else { return }
        let maxY = max(table.bounds.height - scrollView.contentSize.height, 0)
        scrollView.contentView.scroll(to: NSPoint(x: 0, y: min(max(offset, 0), maxY)))
        scrollView.reflectScrolledClipView(scrollView.contentView)
    }
}
