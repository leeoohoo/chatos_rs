import ChatOSCore
import SwiftUI

private struct AgentChatTimelineBottomMarker: View {
    var body: some View {
        Color.clear
            .frame(height: 1)
            .id(AgentChatTimelineViewConstants.bottomID)
    }
}

private enum AgentChatTimelineViewConstants {
    static let bottomID = "agent-chat-timeline-bottom"
    static let rowSpacing: CGFloat = 16
}

/// Only changes that affect message rows may update their hosted content. Scheduler activity,
/// drafts and task-board changes must not recreate historical Markdown documents.
struct AgentChatTimelineRowState: Equatable {
    var profiles: [LocalAgentProfile] = []
    var attachmentThumbnails: [String: AgentMessageAttachmentThumbnail] = [:]
    var teams: [ProjectAgentRoom] = []
    var actionIDs: Set<String> = []
}

enum AgentChatTimelineScrollMetrics {
    static func shouldScrollToBottomInitially(
        markerMaxY: CGFloat,
        viewportHeight: CGFloat
    ) -> Bool {
        guard markerMaxY.isFinite, markerMaxY < CGFloat.greatestFiniteMagnitude / 2,
              viewportHeight > 0 else { return false }
        return markerMaxY > viewportHeight
    }

    static func isAtBottom(
        markerMaxY: CGFloat,
        viewportHeight: CGFloat,
        tolerance: CGFloat = 32
    ) -> Bool {
        guard markerMaxY.isFinite, markerMaxY < CGFloat.greatestFiniteMagnitude / 2,
              viewportHeight > 0 else { return false }
        return markerMaxY <= viewportHeight + tolerance
    }
}

/// The shared timeline for team rooms and direct conversations. The loaded page is intentionally
/// bounded, so a regular VStack is both cheap and deterministic: every message gets its natural
/// height without estimated rows or layout-time geometry feedback.
struct AgentChatTimelineView<Item: Identifiable & Equatable, RowContent: View, EmptyContent: View>: View
where Item.ID == String {
    let items: [Item]
    let isInitialContentReady: Bool
    let hasOlderItems: Bool
    let isLoadingOlderItems: Bool
    let scrollToLatestRequest: Int
    var rowState = AgentChatTimelineRowState()
    let loadOlderItems: () async -> String?
    @ViewBuilder let rowContent: (Item) -> RowContent
    @ViewBuilder let emptyContent: () -> EmptyContent

    @State private var hasPositionedInitially = false
    @State private var isPositioningInitially = false
    @State private var visibleTargetID: String?
    @State private var followsLatest = true

    var body: some View {
        Group {
            if items.isEmpty && !hasOlderItems {
                // An empty state belongs to the viewport, not a height-estimated message row.
                emptyContent()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                timeline
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var timeline: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: AgentChatTimelineViewConstants.rowSpacing) {
                    if hasOlderItems {
                        loadOlderButton(proxy: proxy)
                    }

                    ForEach(items) { item in
                        rowContent(item)
                            .id(item.id)
                    }

                    AgentChatTimelineBottomMarker()
                }
                .scrollTargetLayout()
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(18)
            }
            .defaultScrollAnchor(.bottom)
            .scrollPosition(id: $visibleTargetID, anchor: .bottom)
            .onAppear {
                positionInitially(proxy)
            }
            .onChange(of: isInitialContentReady) {
                positionInitially(proxy)
            }
            .onChange(of: visibleTargetID) { _, targetID in
                followsLatest = targetID == AgentChatTimelineViewConstants.bottomID
            }
            .onChange(of: items.last?.id) {
                guard hasPositionedInitially else {
                    positionInitially(proxy)
                    return
                }
                if followsLatest {
                    proxy.scrollTo(AgentChatTimelineViewConstants.bottomID, anchor: .bottom)
                }
            }
            .onChange(of: scrollToLatestRequest) {
                followsLatest = true
                proxy.scrollTo(AgentChatTimelineViewConstants.bottomID, anchor: .bottom)
            }
        }
    }

    private func loadOlderButton(proxy: ScrollViewProxy) -> some View {
        HStack {
            Spacer()
            Button {
                Task {
                    guard let anchorID = await loadOlderItems() else { return }
                    await Task.yield()
                    proxy.scrollTo(anchorID, anchor: .top)
                }
            } label: {
                if isLoadingOlderItems {
                    ProgressView().controlSize(.small)
                } else {
                    Label("加载更早消息", systemImage: "clock.arrow.circlepath")
                }
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .disabled(isLoadingOlderItems)
            Spacer()
        }
        .padding(.bottom, 4)
    }

    private func positionInitially(_ proxy: ScrollViewProxy) {
        guard !hasPositionedInitially,
              !isPositioningInitially,
              isInitialContentReady,
              !items.isEmpty else { return }
        isPositioningInitially = true
        Task { @MainActor in
            await Task.yield()
            followsLatest = true
            proxy.scrollTo(AgentChatTimelineViewConstants.bottomID, anchor: .bottom)
            hasPositionedInitially = true
            isPositioningInitially = false
        }
    }
}
