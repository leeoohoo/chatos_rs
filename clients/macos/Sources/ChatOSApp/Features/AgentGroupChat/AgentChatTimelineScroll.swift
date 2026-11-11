import ChatOSCore
import SwiftUI

private struct AgentChatTimelineBottomPreferenceKey: PreferenceKey {
    static let defaultValue = CGFloat.greatestFiniteMagnitude

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

private struct AgentChatTimelineBottomMarker: View {
    let coordinateSpaceName: String

    var body: some View {
        GeometryReader { geometry in
            Color.clear.preference(
                key: AgentChatTimelineBottomPreferenceKey.self,
                value: geometry.frame(in: .named(coordinateSpaceName)).maxY
            )
        }
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

/// The shared timeline for team rooms and direct conversations. LazyVStack gives every row its
/// natural content height while only creating rows needed for the visible scroll region.
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
    @State private var isAtBottom = false
    @State private var latestMarkerMaxY = CGFloat.greatestFiniteMagnitude
    @State private var coordinateSpaceName = "agent-chat-timeline-\(UUID().uuidString)"

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
        GeometryReader { viewport in
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: AgentChatTimelineViewConstants.rowSpacing) {
                        if hasOlderItems {
                            loadOlderButton(proxy: proxy)
                        }

                        ForEach(items) { item in
                            rowContent(item)
                                .id(item.id)
                        }

                        AgentChatTimelineBottomMarker(coordinateSpaceName: coordinateSpaceName)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(18)
                }
                .coordinateSpace(name: coordinateSpaceName)
                .onPreferenceChange(AgentChatTimelineBottomPreferenceKey.self) { markerMaxY in
                    if !hasPositionedInitially, latestMarkerMaxY != markerMaxY {
                        latestMarkerMaxY = markerMaxY
                    }
                    let nextValue = AgentChatTimelineScrollMetrics.isAtBottom(
                        markerMaxY: markerMaxY,
                        viewportHeight: viewport.size.height
                    )
                    if nextValue != isAtBottom { isAtBottom = nextValue }
                    positionInitially(proxy, viewportHeight: viewport.size.height)
                }
                .onAppear {
                    positionInitially(proxy, viewportHeight: viewport.size.height)
                }
                .onChange(of: isInitialContentReady) {
                    positionInitially(proxy, viewportHeight: viewport.size.height)
                }
                .onChange(of: viewport.size.height) { _, height in
                    positionInitially(proxy, viewportHeight: height)
                }
                .onChange(of: items.last?.id) {
                    guard hasPositionedInitially else {
                        positionInitially(proxy, viewportHeight: viewport.size.height)
                        return
                    }
                    guard isAtBottom else { return }
                    proxy.scrollTo(AgentChatTimelineViewConstants.bottomID, anchor: .bottom)
                }
                .onChange(of: scrollToLatestRequest) {
                    proxy.scrollTo(AgentChatTimelineViewConstants.bottomID, anchor: .bottom)
                }
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

    private func positionInitially(
        _ proxy: ScrollViewProxy,
        viewportHeight: CGFloat
    ) {
        guard !hasPositionedInitially,
              !isPositioningInitially,
              isInitialContentReady,
              !items.isEmpty,
              latestMarkerMaxY.isFinite,
              latestMarkerMaxY < CGFloat.greatestFiniteMagnitude / 2,
              viewportHeight > 0 else { return }
        isPositioningInitially = true
        Task { @MainActor in
            await Task.yield()
            await Task.yield()
            if AgentChatTimelineScrollMetrics.shouldScrollToBottomInitially(
                markerMaxY: latestMarkerMaxY,
                viewportHeight: viewportHeight
            ) {
                proxy.scrollTo(AgentChatTimelineViewConstants.bottomID, anchor: .bottom)
            }
            hasPositionedInitially = true
            isPositioningInitially = false
        }
    }
}
