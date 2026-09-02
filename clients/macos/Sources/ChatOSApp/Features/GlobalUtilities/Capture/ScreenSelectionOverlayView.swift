import AppKit

@MainActor
final class ScreenSelectionOverlayView: NSView {
    var onSelectionBegan: ((ScreenSelectionOverlayView, NSPoint) -> Void)?
    var onSelectionChanged: ((ScreenSelectionOverlayView, NSPoint) -> Void)?
    var onSelectionCompleted: ((ScreenSelectionOverlayView, NSPoint) -> Void)?
    var onPointerMoved: ((ScreenSelectionOverlayView, NSPoint) -> Void)?
    var onPointerExited: ((ScreenSelectionOverlayView) -> Void)?

    private(set) var selectionRect: NSRect?
    private(set) var hoveredWindowRect: NSRect?
    private(set) var isActiveSelection = false
    private let instructionText: String
    private var trackingArea: NSTrackingArea?

    init(isEnglish: Bool) {
        instructionText = isEnglish
            ? "Hover and click a window, or drag to select an area  ·  Esc to cancel"
            : "悬停并点击窗口，或拖动选择区域  ·  Esc 取消"
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var acceptsFirstResponder: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        window?.acceptsMouseMovedEvents = true
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let trackingArea {
            removeTrackingArea(trackingArea)
        }
        let trackingArea = NSTrackingArea(
            rect: bounds,
            options: [.activeAlways, .mouseMoved, .mouseEnteredAndExited, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(trackingArea)
        self.trackingArea = trackingArea
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func resetCursorRects() {
        addCursorRect(bounds, cursor: .crosshair)
    }

    override func mouseDown(with event: NSEvent) {
        onSelectionBegan?(self, clampedPoint(convert(event.locationInWindow, from: nil)))
    }

    override func mouseDragged(with event: NSEvent) {
        onSelectionChanged?(self, clampedPoint(convert(event.locationInWindow, from: nil)))
    }

    override func mouseUp(with event: NSEvent) {
        onSelectionCompleted?(self, clampedPoint(convert(event.locationInWindow, from: nil)))
    }

    override func mouseMoved(with event: NSEvent) {
        guard !isActiveSelection else { return }
        onPointerMoved?(self, clampedPoint(convert(event.locationInWindow, from: nil)))
    }

    override func mouseExited(with event: NSEvent) {
        guard !isActiveSelection else { return }
        onPointerExited?(self)
    }

    func updateSelection(_ rect: NSRect?, active: Bool) {
        selectionRect = rect
        isActiveSelection = active
        if active {
            hoveredWindowRect = nil
        }
        needsDisplay = true
    }

    func updateHoveredWindow(_ rect: NSRect?) {
        guard !isActiveSelection else { return }
        hoveredWindowRect = rect
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)

        let activeRect = isActiveSelection ? selectionRect : hoveredWindowRect
        let mask = NSBezierPath(rect: bounds)
        if let activeRect, !activeRect.isEmpty {
            mask.appendRect(activeRect)
            mask.windingRule = .evenOdd
        }
        NSColor.black.withAlphaComponent(0.42).setFill()
        mask.fill()

        guard let activeRect,
              activeRect.width > 0,
              activeRect.height > 0 else {
            drawInstruction()
            return
        }

        NSColor.controlAccentColor.setStroke()
        let border = NSBezierPath(rect: activeRect.insetBy(dx: 0.5, dy: 0.5))
        border.lineWidth = 2
        if !isActiveSelection {
            border.setLineDash([6, 4], count: 2, phase: 0)
        }
        border.stroke()
        drawDimensions(for: activeRect)
    }

    private func drawInstruction() {
        let text = instructionText
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 14, weight: .medium),
            .foregroundColor: NSColor.white,
            .backgroundColor: NSColor.black.withAlphaComponent(0.58),
        ]
        let size = text.size(withAttributes: attributes)
        let rect = NSRect(
            x: bounds.midX - size.width / 2 - 12,
            y: bounds.midY - size.height / 2 - 8,
            width: size.width + 24,
            height: size.height + 16
        )
        let background = NSBezierPath(roundedRect: rect, xRadius: 8, yRadius: 8)
        NSColor.black.withAlphaComponent(0.58).setFill()
        background.fill()
        text.draw(
            at: NSPoint(x: rect.minX + 12, y: rect.minY + 8),
            withAttributes: attributes.merging([.backgroundColor: NSColor.clear]) { _, new in new }
        )
    }

    private func drawDimensions(for rect: NSRect) {
        let width = Int(rect.width.rounded())
        let height = Int(rect.height.rounded())
        let text = "\(width) × \(height)"
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedDigitSystemFont(ofSize: 12, weight: .semibold),
            .foregroundColor: NSColor.white,
        ]
        let size = text.size(withAttributes: attributes)
        let badge = NSRect(
            x: min(max(rect.minX, 8), bounds.maxX - size.width - 24),
            y: max(8, rect.minY - size.height - 18),
            width: size.width + 16,
            height: size.height + 8
        )
        NSColor.black.withAlphaComponent(0.72).setFill()
        NSBezierPath(roundedRect: badge, xRadius: 6, yRadius: 6).fill()
        text.draw(
            at: NSPoint(x: badge.minX + 8, y: badge.minY + 4),
            withAttributes: attributes
        )
    }

    private func clampedPoint(_ point: NSPoint) -> NSPoint {
        NSPoint(
            x: min(max(point.x, bounds.minX), bounds.maxX),
            y: min(max(point.y, bounds.minY), bounds.maxY)
        )
    }
}
