import SwiftUI

private struct AgentChatTimelineBottomPreferenceKey: PreferenceKey {
    static let defaultValue = CGFloat.greatestFiniteMagnitude

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

private struct AgentChatTimelineBottomMarker: View {
    let id: String
    let coordinateSpaceName: String

    var body: some View {
        GeometryReader { geometry in
            Color.clear.preference(
                key: AgentChatTimelineBottomPreferenceKey.self,
                value: geometry.frame(in: .named(coordinateSpaceName)).maxY
            )
        }
        .frame(height: 1)
        .id(id)
    }
}

enum AgentChatTimelineScrollMetrics {
    static func isAtBottom(
        markerMaxY: CGFloat,
        viewportHeight: CGFloat,
        tolerance: CGFloat = 32
    ) -> Bool {
        guard markerMaxY < CGFloat.greatestFiniteMagnitude / 2,
              viewportHeight > 0 else { return false }
        return markerMaxY <= viewportHeight + tolerance
    }
}

/// The single scrolling implementation for team rooms and direct conversations. Message and
/// proposal cards are supplied by each surface, while pagination, initial positioning, bottom
/// detection, and follow-latest behavior remain identical.
struct AgentChatTimelineView<Item: Identifiable, RowContent: View, EmptyContent: View>: View
where Item.ID == String {
    let items: [Item]
    let isInitialContentReady: Bool
    let hasOlderItems: Bool
    let isLoadingOlderItems: Bool
    let scrollToLatestRequest: Int
    let loadOlderItems: () async -> String?
    let rowContent: (Item) -> RowContent
    let emptyContent: () -> EmptyContent

    @State private var hasPositionedInitially = false
    @State private var isAtBottom = false
    @State private var coordinateSpaceName = "agent-chat-timeline-\(UUID().uuidString)"

    private let bottomID = "agent-chat-timeline-bottom"

    init(
        items: [Item],
        isInitialContentReady: Bool,
        hasOlderItems: Bool,
        isLoadingOlderItems: Bool,
        scrollToLatestRequest: Int,
        loadOlderItems: @escaping () async -> String?,
        @ViewBuilder rowContent: @escaping (Item) -> RowContent,
        @ViewBuilder emptyContent: @escaping () -> EmptyContent
    ) {
        self.items = items
        self.isInitialContentReady = isInitialContentReady
        self.hasOlderItems = hasOlderItems
        self.isLoadingOlderItems = isLoadingOlderItems
        self.scrollToLatestRequest = scrollToLatestRequest
        self.loadOlderItems = loadOlderItems
        self.rowContent = rowContent
        self.emptyContent = emptyContent
    }

    var body: some View {
        GeometryReader { viewport in
            ScrollViewReader { proxy in
                ScrollView {
                    // The timeline is already bounded by database pagination (20 messages in a
                    // direct chat, 50 in a team room). An eager stack is therefore predictable
                    // in memory and avoids LazyVStack's repeated visible-range placement loop
                    // for very tall native Markdown views.
                    VStack(alignment: .leading, spacing: 12) {
                        if hasOlderItems {
                            HStack {
                                Spacer()
                                Button {
                                    Task {
                                        if let anchorID = await loadOlderItems() {
                                            await Task.yield()
                                            proxy.scrollTo(anchorID, anchor: .top)
                                        }
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

                        if items.isEmpty {
                            emptyContent()
                        } else {
                            ForEach(items) { item in
                                rowContent(item).id(item.id)
                            }
                        }

                        AgentChatTimelineBottomMarker(
                            id: bottomID,
                            coordinateSpaceName: coordinateSpaceName
                        )
                    }
                    .padding(18)
                }
                .agentChatInitialScrollAnchor()
                .coordinateSpace(name: coordinateSpaceName)
                .onPreferenceChange(AgentChatTimelineBottomPreferenceKey.self) { markerMaxY in
                    let nextValue = AgentChatTimelineScrollMetrics.isAtBottom(
                        markerMaxY: markerMaxY,
                        viewportHeight: viewport.size.height
                    )
                    if nextValue != isAtBottom { isAtBottom = nextValue }
                }
                .onAppear { positionInitially(proxy) }
                .onChange(of: isInitialContentReady) { positionInitially(proxy) }
                .onChange(of: items.last?.id) {
                    guard hasPositionedInitially else {
                        positionInitially(proxy)
                        return
                    }
                    guard isAtBottom else { return }
                    withAnimation { proxy.scrollTo(bottomID, anchor: .bottom) }
                }
                .onChange(of: scrollToLatestRequest) {
                    withAnimation { proxy.scrollTo(bottomID, anchor: .bottom) }
                }
            }
        }
    }

    private func positionInitially(_ proxy: ScrollViewProxy) {
        guard !hasPositionedInitially,
              isInitialContentReady,
              !items.isEmpty else { return }
        hasPositionedInitially = true
        if #available(macOS 15.0, *) {
            // The role-specific anchors position long content at its latest item while keeping
            // short content top-aligned. No post-layout jump is required on current systems.
            return
        }
        Task { @MainActor in
            await Task.yield()
            // A short transcript cannot scroll, so it remains naturally top-aligned. A long
            // transcript opens at the latest item without using a bottom default anchor.
            proxy.scrollTo(bottomID, anchor: .bottom)
        }
    }
}

private extension View {
    @ViewBuilder
    func agentChatInitialScrollAnchor() -> some View {
        if #available(macOS 15.0, *) {
            self
                .defaultScrollAnchor(.bottom, for: .initialOffset)
                .defaultScrollAnchor(.top, for: .alignment)
        } else {
            self
        }
    }
}
