@preconcurrency import AppKit
import ChatOSCore
import Combine
import SwiftUI
struct PetStackedPanelPlacement {
    static func origin(
        size: NSSize,
        anchorFrame: NSRect,
        visibleFrame: NSRect
    ) -> NSPoint {
        let inset: CGFloat = 8
        let gap: CGFloat = 10
        let minimumX = visibleFrame.minX + inset
        let maximumX = visibleFrame.maxX - size.width - inset
        let centeredX = anchorFrame.midX - size.width / 2
        let x = min(max(centeredX, minimumX), maximumX)

        let aboveY = anchorFrame.maxY + gap
        if aboveY + size.height <= visibleFrame.maxY - inset {
            return NSPoint(x: x, y: aboveY)
        }

        let belowY = anchorFrame.minY - size.height - gap
        if belowY >= visibleFrame.minY + inset {
            return NSPoint(x: x, y: belowY)
        }

        let verticalY = min(
            max(anchorFrame.maxY - size.height, visibleFrame.minY + inset),
            visibleFrame.maxY - size.height - inset
        )
        let rightX = anchorFrame.maxX + gap
        if rightX + size.width <= visibleFrame.maxX - inset {
            return NSPoint(x: rightX, y: verticalY)
        }

        let leftX = anchorFrame.minX - size.width - gap
        if leftX >= visibleFrame.minX + inset {
            return NSPoint(x: leftX, y: verticalY)
        }

        return NSPoint(x: x, y: verticalY)
    }
}

struct PetTaskInspectorPlacement {
    struct Layout: Equatable {
        let conversationOrigin: NSPoint
        let inspectorOrigin: NSPoint
    }

    static func layout(
        size: NSSize,
        conversationFrame: NSRect,
        visibleFrame: NSRect
    ) -> Layout {
        let screenInset: CGFloat = 8
        let gap: CGFloat = 12
        let minimumX = visibleFrame.minX + screenInset
        let maximumConversationX = visibleFrame.maxX
            - conversationFrame.width
            - screenInset
        let requiredConversationX = minimumX + size.width + gap
        let conversationX = min(
            max(conversationFrame.minX, requiredConversationX),
            maximumConversationX
        )
        let inspectorX = max(minimumX, conversationX - size.width - gap)
        let preferredInspectorY = conversationFrame.maxY - size.height
        let inspectorY = min(
            max(preferredInspectorY, visibleFrame.minY + screenInset),
            visibleFrame.maxY - size.height - screenInset
        )
        return Layout(
            conversationOrigin: NSPoint(
                x: conversationX,
                y: conversationFrame.minY
            ),
            inspectorOrigin: NSPoint(x: inspectorX, y: inspectorY)
        )
    }
}

final class PetMessagePanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .leftMouseDown, !isKeyWindow {
            makeKey()
        }
        super.sendEvent(event)
    }
}

final class PetTaskInspectorPanel: NSPanel {
    var onCancel: (() -> Void)?

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func cancelOperation(_ sender: Any?) {
        onCancel?()
    }
}

final class PetFileWorkbenchPanel: NSPanel {
    var onCancel: (() -> Void)?

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func cancelOperation(_ sender: Any?) {
        onCancel?()
    }
}

struct PetLocalizedRoot<Content: View>: View {
    @ObservedObject var model: AppModel
    let content: Content

    var body: some View {
        content
            .environmentObject(model)
            .environment(\.locale, model.interfaceLocale)
            .environment(\.interfaceFontScale, model.interfaceFontScale)
            .dynamicTypeSize(model.interfaceDynamicTypeSize)
    }
}

@MainActor
final class PetInteractionHostingView<Content: View>: NSHostingView<Content> {
    private let onInteractionBegan: () -> Void
    private let onInteractionEnded: (Bool) -> Void
    private var dragStartMouseLocation: NSPoint?
    private var dragStartWindowOrigin: NSPoint?
    private var didMoveWindow = false

    init(
        rootView: Content,
        onInteractionBegan: @escaping () -> Void,
        onInteractionEnded: @escaping (Bool) -> Void
    ) {
        self.onInteractionBegan = onInteractionBegan
        self.onInteractionEnded = onInteractionEnded
        super.init(rootView: rootView)
        wantsLayer = true
        layer?.isOpaque = false
        layer?.backgroundColor = NSColor.clear.cgColor
    }

    @available(*, unavailable)
    required init(rootView: Content) {
        fatalError("init(rootView:) is unavailable")
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var mouseDownCanMoveWindow: Bool { false }

    override var isOpaque: Bool { false }

    override func mouseDown(with event: NSEvent) {
        guard let window else {
            super.mouseDown(with: event)
            return
        }
        // Do not use NSWindow.performDrag here. Its modal event-tracking loop
        // prevents SwiftUI's TimelineView from reliably advancing sprite frames
        // until the pointer is released. Moving the panel from mouseDragged keeps
        // the normal run loop alive, so the directional gait animates in real time.
        dragStartMouseLocation = NSEvent.mouseLocation
        dragStartWindowOrigin = window.frame.origin
        didMoveWindow = false
        onInteractionBegan()
    }

    override func mouseDragged(with event: NSEvent) {
        guard let window,
              let startMouseLocation = dragStartMouseLocation,
              let startWindowOrigin = dragStartWindowOrigin else {
            super.mouseDragged(with: event)
            return
        }
        let currentMouseLocation = NSEvent.mouseLocation
        let deltaX = currentMouseLocation.x - startMouseLocation.x
        let deltaY = currentMouseLocation.y - startMouseLocation.y
        window.setFrameOrigin(NSPoint(
            x: startWindowOrigin.x + deltaX,
            y: startWindowOrigin.y + deltaY
        ))
        if !didMoveWindow, hypot(deltaX, deltaY) >= 3 {
            didMoveWindow = true
        }
    }

    override func mouseUp(with event: NSEvent) {
        guard let window,
              let startMouseLocation = dragStartMouseLocation,
              let startWindowOrigin = dragStartWindowOrigin else {
            super.mouseUp(with: event)
            return
        }
        let currentMouseLocation = NSEvent.mouseLocation
        let mouseDistance = hypot(
            currentMouseLocation.x - startMouseLocation.x,
            currentMouseLocation.y - startMouseLocation.y
        )
        let windowDistance = hypot(
            window.frame.origin.x - startWindowOrigin.x,
            window.frame.origin.y - startWindowOrigin.y
        )
        dragStartMouseLocation = nil
        dragStartWindowOrigin = nil
        let completedDrag = didMoveWindow || max(mouseDistance, windowDistance) >= 3
        didMoveWindow = false
        onInteractionEnded(completedDrag)
    }
}
