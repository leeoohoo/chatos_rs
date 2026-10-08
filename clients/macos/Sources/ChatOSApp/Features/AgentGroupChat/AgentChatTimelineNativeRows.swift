import AppKit
import SwiftUI

/// Type-erasure preserves value equality without evaluating a SwiftUI row off screen.
struct AgentChatTimelineEntry {
    let id: String
    private let value: Any
    private let equals: (Any) -> Bool
    let content: () -> AnyView

    init<Value: Equatable>(id: String, value: Value, content: @escaping () -> AnyView) {
        self.id = id
        self.value = value
        equals = { ($0 as? Value) == value }
        self.content = content
    }

    func hasSameContent(as other: Self) -> Bool {
        id == other.id && equals(other.value)
    }
}

struct AgentChatTimelineHeightCache {
    static let capacity = 512
    private var values: [String: CGFloat] = [:]
    private var order: [String] = []

    var count: Int { values.count }

    func height(for id: String) -> CGFloat { values[id] ?? 140 }

    mutating func store(_ height: CGFloat, for id: String) -> Bool {
        guard height.isFinite, height > 0 else { return false }
        let nextHeight = max(ceil(height), 1)
        guard abs((values[id] ?? 140) - nextHeight) > 0.5 else { return false }
        if values[id] == nil { order.append(id) }
        values[id] = nextHeight
        while order.count > Self.capacity {
            values.removeValue(forKey: order.removeFirst())
        }
        return true
    }

    mutating func invalidate(_ id: String) {
        values.removeValue(forKey: id)
        order.removeAll { $0 == id }
    }

    mutating func removeAll() {
        values.removeAll(keepingCapacity: true)
        order.removeAll(keepingCapacity: true)
    }
}

/// Measure natural height at the actual column width, independently of the table's estimate.
/// Only a change in size is reported; scrolling never updates this SwiftUI measurement.
private struct AgentChatMeasuredRow: View {
    let content: AnyView
    let width: CGFloat
    let fontScale: CGFloat
    let colorScheme: ColorScheme
    let reportHeight: (CGFloat) -> Void

    var body: some View {
        content
            .environment(\.interfaceFontScale, fontScale)
            .environment(\.colorScheme, colorScheme)
            .frame(width: max(width - 36, 1), alignment: .leading)
            .padding(.horizontal, 18)
            .padding(.vertical, 6)
            .fixedSize(horizontal: false, vertical: true)
            .onGeometryChange(for: CGSize.self) { $0.size } action: { reportHeight($0.height) }
    }
}

@MainActor
final class AgentChatTimelineCell: NSTableCellView {
    private var hostingView: NSHostingView<AnyView>?
    private(set) var representedID: String?

    func configure(
        entry: AgentChatTimelineEntry,
        width: CGFloat,
        fontScale: CGFloat,
        colorScheme: ColorScheme,
        reportHeight: @escaping (CGFloat) -> Void
    ) {
        representedID = entry.id
        let root = AnyView(AgentChatMeasuredRow(
            content: entry.content(), width: width, fontScale: fontScale,
            colorScheme: colorScheme, reportHeight: reportHeight
        ).id(entry.id))
        if let hostingView {
            hostingView.rootView = root
        } else {
            let hostingView = NSHostingView(rootView: root)
            hostingView.sizingOptions = []
            hostingView.autoresizingMask = [.width, .height]
            hostingView.frame = bounds
            addSubview(hostingView)
            self.hostingView = hostingView
        }
    }
}

@MainActor
final class AgentChatTimelineScrollView: NSScrollView {
    var resized: (() -> Void)?
    private var previousSize = NSSize.zero

    override func layout() {
        super.layout()
        guard contentSize != previousSize else { return }
        previousSize = contentSize
        resized?()
    }
}
