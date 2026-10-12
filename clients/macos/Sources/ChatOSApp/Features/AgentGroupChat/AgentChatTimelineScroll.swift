import ChatOSCore
import SwiftUI

/// Only changes that affect message rows may update their hosted content. Scheduler activity,
/// drafts and task-board changes must not recreate historical Markdown documents.
struct AgentChatTimelineRowState: Equatable {
    var profiles: [LocalAgentProfile] = []
    var attachmentData: [String: Data] = [:]
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

/// The native table creates/recycles views only for visible rows. Loaded history remains cheap
/// metadata; scrolling no longer feeds positions through SwiftUI or lays out a whole page.
struct AgentChatTimelineView<Item: Identifiable & Equatable, RowContent: View, EmptyContent: View>: View
where Item.ID == String {
    @Environment(\.interfaceFontScale) private var fontScale
    @Environment(\.colorScheme) private var colorScheme
    let items: [Item]
    let isInitialContentReady: Bool
    let hasOlderItems: Bool
    let isLoadingOlderItems: Bool
    let scrollToLatestRequest: Int
    var rowState = AgentChatTimelineRowState()
    let loadOlderItems: () async -> String?
    @ViewBuilder let rowContent: (Item) -> RowContent
    @ViewBuilder let emptyContent: () -> EmptyContent

    var body: some View {
        Group {
            if items.isEmpty && !hasOlderItems {
                // An empty state belongs to the viewport, not a height-estimated message row.
                emptyContent()
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                AgentChatNativeTimeline(
                    entries: entries,
                    rowState: rowState,
                    fontScale: fontScale,
                    colorScheme: colorScheme,
                    isInitialContentReady: isInitialContentReady && !items.isEmpty,
                    scrollToLatestRequest: scrollToLatestRequest
                )
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var entries: [AgentChatTimelineEntry] {
        var result: [AgentChatTimelineEntry] = []
        if hasOlderItems {
            let loading = isLoadingOlderItems
            result.append(.init(id: "timeline:load-older", value: loading) {
                AnyView(HStack {
                    Spacer()
                    Button {
                        Task { _ = await loadOlderItems() }
                    } label: {
                        if loading {
                            ProgressView().controlSize(.small)
                        } else {
                            Label("加载更早消息", systemImage: "clock.arrow.circlepath")
                        }
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                    .disabled(loading)
                    Spacer()
                }.padding(.vertical, 8))
            })
        }
        result += items.map { item in
            AgentChatTimelineEntry(id: "timeline:item:\(item.id)", value: item) {
                AnyView(rowContent(item))
            }
        }
        return result
    }
}
