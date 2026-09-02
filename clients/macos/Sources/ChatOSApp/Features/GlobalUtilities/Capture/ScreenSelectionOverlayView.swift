import AppKit

@MainActor
final class ScreenSelectionOverlayView: NSView {
    var onSelectionBegan: ((ScreenSelectionOverlayView, NSPoint) -> Void)?
    var onSelectionChanged: ((ScreenSelectionOverlayView, NSPoint) -> Void)?
    var onSelectionCompleted: ((ScreenSelectionOverlayView, NSPoint) -> Void)?

    private(set) var selectionRect: NSRect?
    private(set) var isActiveSelection = false
    private let instructionText: String

    init(isEnglish: Bool) {
        instructionText = isEnglish
            ? "Drag to select an area  ·  Esc to cancel"
            : "拖动选择截图区域  ·  Esc 取消"
        super.init(frame: .zero)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var acceptsFirstResponder: Bool { true }

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

    func updateSelection(_ rect: NSRect?, active: Bool) {
        selectionRect = rect
        isActiveSelection = active
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)

        let mask = NSBezierPath(rect: bounds)
        if let selectionRect, !selectionRect.isEmpty {
            mask.appendRect(selectionRect)
            mask.windingRule = .evenOdd
        }
        NSColor.black.withAlphaComponent(0.42).setFill()
        mask.fill()

        guard let selectionRect,
              selectionRect.width > 0,
              selectionRect.height > 0 else {
            drawInstruction()
            return
        }

        NSColor.controlAccentColor.setStroke()
        let border = NSBezierPath(rect: selectionRect.insetBy(dx: 0.5, dy: 0.5))
        border.lineWidth = 2
        border.stroke()
        drawDimensions(for: selectionRect)
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
