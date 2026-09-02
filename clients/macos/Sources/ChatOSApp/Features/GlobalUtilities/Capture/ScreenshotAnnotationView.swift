import AppKit
import CoreGraphics

enum ScreenshotAnnotationTool: Int {
    case pen
    case line
    case rectangle
    case ellipse
    case arrow
    case highlight
    case mosaic
    case text
    case number

    static func shortcut(_ value: String) -> ScreenshotAnnotationTool? {
        switch value.lowercased() {
        case "1", "p": .pen
        case "2", "l": .line
        case "3", "r": .rectangle
        case "4", "o": .ellipse
        case "5", "a": .arrow
        case "6", "h": .highlight
        case "7", "m": .mosaic
        case "8", "t": .text
        case "9", "n": .number
        default: nil
        }
    }
}

private enum ScreenshotAnnotation {
    case pen(points: [CGPoint], color: NSColor, lineWidth: CGFloat)
    case line(start: CGPoint, end: CGPoint, color: NSColor, lineWidth: CGFloat)
    case rectangle(rect: CGRect, color: NSColor, lineWidth: CGFloat)
    case ellipse(rect: CGRect, color: NSColor, lineWidth: CGFloat)
    case arrow(start: CGPoint, end: CGPoint, color: NSColor, lineWidth: CGFloat)
    case highlight(rect: CGRect, color: NSColor)
    case mosaic(rect: CGRect)
    case text(origin: CGPoint, value: String, color: NSColor, fontSize: CGFloat)
    case number(center: CGPoint, value: Int, color: NSColor, diameter: CGFloat)
}

@MainActor
final class ScreenshotAnnotationView: NSView, NSTextFieldDelegate {
    var tool: ScreenshotAnnotationTool = .pen {
        didSet {
            if oldValue != tool {
                commitActiveTextEditing()
            }
        }
    }
    var annotationColor: NSColor = .systemRed
    var lineWidthScale: CGFloat = 1
    var onAnnotationsChanged: (() -> Void)?

    private let sourceImage: CGImage
    private let displayImage: NSImage
    private let mosaicImage: NSImage
    private var annotations: [ScreenshotAnnotation] = []
    private var redoAnnotations: [ScreenshotAnnotation] = []
    private var workingAnnotation: ScreenshotAnnotation?
    private var dragOrigin: CGPoint?
    private weak var activeTextField: NSTextField?
    private var activeTextOrigin: CGPoint?

    init(image: CGImage) {
        sourceImage = image
        displayImage = NSImage(
            cgImage: image,
            size: NSSize(width: image.width, height: image.height)
        )
        mosaicImage = Self.makeMosaicImage(from: image)
        super.init(frame: .zero)
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    var canUndo: Bool {
        !annotations.isEmpty
    }

    var canRedo: Bool {
        !redoAnnotations.isEmpty
    }

    var hasAnnotations: Bool {
        !annotations.isEmpty || activeTextField != nil
    }

    var isEditingText: Bool {
        activeTextField != nil
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func resetCursorRects() {
        addCursorRect(imageFrame, cursor: tool == .pen ? .crosshair : .crosshair)
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let frame = imageFrame
        guard frame.width > 0, frame.height > 0 else { return }

        displayImage.draw(
            in: frame,
            from: .zero,
            operation: .sourceOver,
            fraction: 1,
            respectFlipped: true,
            hints: [.interpolation: NSImageInterpolation.high]
        )
        for annotation in annotations {
            draw(annotation, in: frame)
        }
        if let workingAnnotation {
            draw(workingAnnotation, in: frame)
        }
    }

    override func mouseDown(with event: NSEvent) {
        let viewPoint = convert(event.locationInWindow, from: nil)
        guard imageFrame.contains(viewPoint) else { return }
        if tool == .text {
            beginTextEditing(at: viewPoint)
            return
        }
        commitActiveTextEditing()
        let point = imagePoint(from: viewPoint)
        dragOrigin = point
        switch tool {
        case .pen:
            workingAnnotation = .pen(
                points: [point],
                color: annotationColor,
                lineWidth: activeLineWidth
            )
        case .line:
            workingAnnotation = .line(
                start: point,
                end: point,
                color: annotationColor,
                lineWidth: activeLineWidth
            )
        case .rectangle:
            workingAnnotation = .rectangle(
                rect: CGRect(origin: point, size: .zero),
                color: annotationColor,
                lineWidth: activeLineWidth
            )
        case .ellipse:
            workingAnnotation = .ellipse(
                rect: CGRect(origin: point, size: .zero),
                color: annotationColor,
                lineWidth: activeLineWidth
            )
        case .arrow:
            workingAnnotation = .arrow(
                start: point,
                end: point,
                color: annotationColor,
                lineWidth: activeLineWidth
            )
        case .highlight:
            workingAnnotation = .highlight(
                rect: CGRect(origin: point, size: .zero),
                color: annotationColor
            )
        case .mosaic:
            workingAnnotation = .mosaic(rect: CGRect(origin: point, size: .zero))
        case .text:
            break
        case .number:
            annotations.append(.number(
                center: point,
                value: nextNumber,
                color: annotationColor,
                diameter: defaultNumberDiameter * lineWidthScale
            ))
            redoAnnotations.removeAll()
            dragOrigin = nil
            workingAnnotation = nil
            needsDisplay = true
            onAnnotationsChanged?()
        }
        needsDisplay = true
    }

    override func mouseDragged(with event: NSEvent) {
        guard let dragOrigin else { return }
        let point = imagePoint(from: clampedToImage(convert(event.locationInWindow, from: nil)))
        switch workingAnnotation {
        case let .pen(points, color, lineWidth):
            var updated = points
            if updated.last.map({ hypot($0.x - point.x, $0.y - point.y) >= 0.75 }) ?? true {
                updated.append(point)
            }
            workingAnnotation = .pen(points: updated, color: color, lineWidth: lineWidth)
        case let .line(start, _, color, lineWidth):
            workingAnnotation = .line(
                start: start,
                end: point,
                color: color,
                lineWidth: lineWidth
            )
        case let .rectangle(_, color, lineWidth):
            workingAnnotation = .rectangle(
                rect: normalizedRect(from: dragOrigin, to: point),
                color: color,
                lineWidth: lineWidth
            )
        case let .ellipse(_, color, lineWidth):
            workingAnnotation = .ellipse(
                rect: normalizedRect(from: dragOrigin, to: point),
                color: color,
                lineWidth: lineWidth
            )
        case let .arrow(start, _, color, lineWidth):
            workingAnnotation = .arrow(
                start: start,
                end: point,
                color: color,
                lineWidth: lineWidth
            )
        case let .highlight(_, color):
            workingAnnotation = .highlight(
                rect: normalizedRect(from: dragOrigin, to: point),
                color: color
            )
        case .mosaic:
            workingAnnotation = .mosaic(
                rect: normalizedRect(from: dragOrigin, to: point)
            )
        case .text, .number:
            break
        case nil:
            break
        }
        needsDisplay = true
    }

    override func mouseUp(with event: NSEvent) {
        guard dragOrigin != nil, let workingAnnotation else { return }
        mouseDragged(with: event)

        let shouldKeep: Bool
        switch self.workingAnnotation ?? workingAnnotation {
        case let .pen(points, _, _):
            shouldKeep = !points.isEmpty
        case let .line(start, end, _, _):
            shouldKeep = hypot(end.x - start.x, end.y - start.y) >= 4
        case let .rectangle(rect, _, _):
            shouldKeep = rect.width >= 2 && rect.height >= 2
        case let .ellipse(rect, _, _):
            shouldKeep = rect.width >= 2 && rect.height >= 2
        case let .arrow(start, end, _, _):
            shouldKeep = hypot(end.x - start.x, end.y - start.y) >= 4
        case let .highlight(rect, _), let .mosaic(rect):
            shouldKeep = rect.width >= 2 && rect.height >= 2
        case .text, .number:
            shouldKeep = false
        }
        if shouldKeep, let completed = self.workingAnnotation {
            annotations.append(completed)
            redoAnnotations.removeAll()
        }
        self.workingAnnotation = nil
        dragOrigin = nil
        needsDisplay = true
        onAnnotationsChanged?()
    }

    func undo() {
        if activeTextField != nil {
            cancelActiveTextEditing()
            return
        }
        guard !annotations.isEmpty else { return }
        redoAnnotations.append(annotations.removeLast())
        needsDisplay = true
        onAnnotationsChanged?()
    }

    func redo() {
        guard let annotation = redoAnnotations.popLast() else { return }
        annotations.append(annotation)
        needsDisplay = true
        onAnnotationsChanged?()
    }

    func clear() {
        guard !annotations.isEmpty || workingAnnotation != nil || activeTextField != nil else {
            return
        }
        cancelActiveTextEditing(notify: false)
        annotations.removeAll()
        redoAnnotations.removeAll()
        workingAnnotation = nil
        dragOrigin = nil
        needsDisplay = true
        onAnnotationsChanged?()
    }

    func renderedImage() -> CGImage? {
        commitActiveTextEditing()
        let width = sourceImage.width
        let height = sourceImage.height
        guard let representation = NSBitmapImageRep(
            bitmapDataPlanes: nil,
            pixelsWide: width,
            pixelsHigh: height,
            bitsPerSample: 8,
            samplesPerPixel: 4,
            hasAlpha: true,
            isPlanar: false,
            colorSpaceName: .deviceRGB,
            bytesPerRow: 0,
            bitsPerPixel: 0
        ), let graphicsContext = NSGraphicsContext(bitmapImageRep: representation) else {
            return nil
        }

        let size = NSSize(width: width, height: height)
        representation.size = size
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = graphicsContext
        graphicsContext.imageInterpolation = .high
        displayImage.draw(in: NSRect(origin: .zero, size: size))
        for annotation in annotations {
            draw(annotation, in: NSRect(origin: .zero, size: size))
        }
        graphicsContext.flushGraphics()
        NSGraphicsContext.restoreGraphicsState()
        return representation.cgImage
    }

    private var defaultLineWidth: CGFloat {
        max(3, CGFloat(sourceImage.width) / 360)
    }

    private var activeLineWidth: CGFloat {
        defaultLineWidth * min(max(lineWidthScale, 0.5), 3)
    }

    private var defaultNumberDiameter: CGFloat {
        max(32, CGFloat(sourceImage.width) / 28)
    }

    private var nextNumber: Int {
        let used = annotations.compactMap { annotation -> Int? in
            guard case let .number(_, value, _, _) = annotation else { return nil }
            return value
        }
        return (used.max() ?? 0) + 1
    }

    private var defaultTextFontSize: CGFloat {
        guard imageFrame.width > 0 else { return 24 }
        return 18 * CGFloat(sourceImage.width) / imageFrame.width
    }

    private var imageFrame: NSRect {
        bounds
    }

    private func imagePoint(from viewPoint: CGPoint) -> CGPoint {
        let frame = imageFrame
        return CGPoint(
            x: (viewPoint.x - frame.minX) / frame.width * CGFloat(sourceImage.width),
            y: (viewPoint.y - frame.minY) / frame.height * CGFloat(sourceImage.height)
        )
    }

    private func viewPoint(from imagePoint: CGPoint, in frame: NSRect) -> CGPoint {
        CGPoint(
            x: frame.minX + imagePoint.x / CGFloat(sourceImage.width) * frame.width,
            y: frame.minY + imagePoint.y / CGFloat(sourceImage.height) * frame.height
        )
    }

    private func clampedToImage(_ point: CGPoint) -> CGPoint {
        let frame = imageFrame
        return CGPoint(
            x: min(max(point.x, frame.minX), frame.maxX),
            y: min(max(point.y, frame.minY), frame.maxY)
        )
    }

    private func draw(_ annotation: ScreenshotAnnotation, in frame: NSRect) {
        let scale = frame.width / CGFloat(sourceImage.width)
        switch annotation {
        case let .pen(points, color, lineWidth):
            guard let first = points.first else { return }
            let path = NSBezierPath()
            path.move(to: viewPoint(from: first, in: frame))
            for point in points.dropFirst() {
                path.line(to: viewPoint(from: point, in: frame))
            }
            if points.count == 1 {
                let center = viewPoint(from: first, in: frame)
                let radius = max(1.5, lineWidth * scale / 2)
                color.setFill()
                NSBezierPath(ovalIn: NSRect(
                    x: center.x - radius,
                    y: center.y - radius,
                    width: radius * 2,
                    height: radius * 2
                )).fill()
            } else {
                color.setStroke()
                path.lineWidth = max(1.5, lineWidth * scale)
                path.lineCapStyle = .round
                path.lineJoinStyle = .round
                path.stroke()
            }
        case let .line(start, end, color, lineWidth):
            let path = NSBezierPath()
            path.move(to: viewPoint(from: start, in: frame))
            path.line(to: viewPoint(from: end, in: frame))
            path.lineWidth = max(1.5, lineWidth * scale)
            path.lineCapStyle = .round
            color.setStroke()
            path.stroke()
        case let .rectangle(rect, color, lineWidth):
            let start = viewPoint(from: rect.origin, in: frame)
            let end = viewPoint(
                from: CGPoint(x: rect.maxX, y: rect.maxY),
                in: frame
            )
            let viewRect = normalizedRect(from: start, to: end)
            color.setStroke()
            let path = NSBezierPath(rect: viewRect)
            path.lineWidth = max(1.5, lineWidth * scale)
            path.lineJoinStyle = .round
            path.stroke()
        case let .ellipse(rect, color, lineWidth):
            let start = viewPoint(from: rect.origin, in: frame)
            let end = viewPoint(
                from: CGPoint(x: rect.maxX, y: rect.maxY),
                in: frame
            )
            color.setStroke()
            let path = NSBezierPath(ovalIn: normalizedRect(from: start, to: end))
            path.lineWidth = max(1.5, lineWidth * scale)
            path.stroke()
        case let .arrow(start, end, color, lineWidth):
            drawArrow(
                from: viewPoint(from: start, in: frame),
                to: viewPoint(from: end, in: frame),
                color: color,
                lineWidth: max(1.5, lineWidth * scale)
            )
        case let .highlight(rect, color):
            let start = viewPoint(from: rect.origin, in: frame)
            let end = viewPoint(
                from: CGPoint(x: rect.maxX, y: rect.maxY),
                in: frame
            )
            color.withAlphaComponent(0.28).setFill()
            NSBezierPath(
                roundedRect: normalizedRect(from: start, to: end),
                xRadius: 4,
                yRadius: 4
            ).fill()
        case let .mosaic(rect):
            let start = viewPoint(from: rect.origin, in: frame)
            let end = viewPoint(
                from: CGPoint(x: rect.maxX, y: rect.maxY),
                in: frame
            )
            NSGraphicsContext.saveGraphicsState()
            NSBezierPath(rect: normalizedRect(from: start, to: end)).addClip()
            mosaicImage.draw(
                in: frame,
                from: .zero,
                operation: .sourceOver,
                fraction: 1,
                respectFlipped: true,
                hints: [.interpolation: NSImageInterpolation.none]
            )
            NSGraphicsContext.restoreGraphicsState()
        case let .text(origin, value, color, fontSize):
            let point = viewPoint(from: origin, in: frame)
            value.draw(
                at: point,
                withAttributes: [
                    .font: NSFont.systemFont(
                        ofSize: max(10, fontSize * scale),
                        weight: .semibold
                    ),
                    .foregroundColor: color,
                ]
            )
        case let .number(center, value, color, diameter):
            let point = viewPoint(from: center, in: frame)
            let renderedDiameter = max(22, diameter * scale)
            let circleRect = NSRect(
                x: point.x - renderedDiameter / 2,
                y: point.y - renderedDiameter / 2,
                width: renderedDiameter,
                height: renderedDiameter
            )
            color.setFill()
            NSBezierPath(ovalIn: circleRect).fill()
            let text = String(value)
            let attributes: [NSAttributedString.Key: Any] = [
                .font: NSFont.monospacedDigitSystemFont(
                    ofSize: max(12, renderedDiameter * 0.5),
                    weight: .bold
                ),
                .foregroundColor: NSColor.white,
            ]
            let size = text.size(withAttributes: attributes)
            text.draw(
                at: NSPoint(x: circleRect.midX - size.width / 2, y: circleRect.midY - size.height / 2),
                withAttributes: attributes
            )
        }
    }

    private func drawArrow(
        from start: CGPoint,
        to end: CGPoint,
        color: NSColor,
        lineWidth: CGFloat
    ) {
        let dx = end.x - start.x
        let dy = end.y - start.y
        let length = hypot(dx, dy)
        guard length > 0 else { return }

        let line = NSBezierPath()
        line.move(to: start)
        line.line(to: end)
        line.lineWidth = lineWidth
        line.lineCapStyle = .round
        line.lineJoinStyle = .round
        color.setStroke()
        line.stroke()

        let angle = atan2(dy, dx)
        let headLength = min(length * 0.42, max(12, lineWidth * 4.5))
        let spread = CGFloat.pi / 6
        let left = CGPoint(
            x: end.x - headLength * cos(angle - spread),
            y: end.y - headLength * sin(angle - spread)
        )
        let right = CGPoint(
            x: end.x - headLength * cos(angle + spread),
            y: end.y - headLength * sin(angle + spread)
        )
        let head = NSBezierPath()
        head.move(to: left)
        head.line(to: end)
        head.line(to: right)
        head.lineWidth = lineWidth
        head.lineCapStyle = .round
        head.lineJoinStyle = .round
        head.stroke()
    }

    private func beginTextEditing(at point: CGPoint) {
        commitActiveTextEditing()
        let width = min(280, max(120, bounds.maxX - point.x))
        let origin = CGPoint(
            x: min(max(point.x, bounds.minX), bounds.maxX - width),
            y: min(max(point.y - 4, bounds.minY), bounds.maxY - 30)
        )
        let field = NSTextField(frame: NSRect(
            origin: origin,
            size: NSSize(width: width, height: 30)
        ))
        field.font = .systemFont(ofSize: 18, weight: .semibold)
        field.textColor = annotationColor
        field.backgroundColor = NSColor.windowBackgroundColor.withAlphaComponent(0.92)
        field.drawsBackground = true
        field.isBordered = true
        field.bezelStyle = .roundedBezel
        field.focusRingType = .none
        field.placeholderString = "Text"
        field.delegate = self
        field.target = self
        field.action = #selector(textFieldSubmitted(_:))
        addSubview(field)
        activeTextField = field
        activeTextOrigin = imagePoint(from: origin)
        window?.makeKey()
        window?.makeFirstResponder(field)
        onAnnotationsChanged?()
    }

    @objc private func textFieldSubmitted(_ sender: NSTextField) {
        commitActiveTextEditing()
    }

    func control(
        _ control: NSControl,
        textView: NSTextView,
        doCommandBy commandSelector: Selector
    ) -> Bool {
        if commandSelector == #selector(NSResponder.insertNewline(_:)) {
            commitActiveTextEditing()
            return true
        }
        if commandSelector == #selector(NSResponder.cancelOperation(_:)) {
            cancelActiveTextEditing()
            return true
        }
        return false
    }

    func controlTextDidEndEditing(_ notification: Notification) {
        commitActiveTextEditing()
    }

    private func commitActiveTextEditing() {
        guard let field = activeTextField else { return }
        let origin = activeTextOrigin ?? .zero
        let color = field.textColor ?? annotationColor
        let value = field.stringValue.trimmingCharacters(in: .whitespacesAndNewlines)
        activeTextField = nil
        activeTextOrigin = nil
        field.delegate = nil
        field.removeFromSuperview()
        if !value.isEmpty {
            annotations.append(.text(
                origin: origin,
                value: value,
                color: color,
                fontSize: defaultTextFontSize
            ))
            redoAnnotations.removeAll()
        }
        needsDisplay = true
        onAnnotationsChanged?()
    }

    private func cancelActiveTextEditing(notify: Bool = true) {
        guard let field = activeTextField else { return }
        activeTextField = nil
        activeTextOrigin = nil
        field.delegate = nil
        field.removeFromSuperview()
        if notify {
            onAnnotationsChanged?()
        }
    }

    private func normalizedRect(from start: CGPoint, to end: CGPoint) -> CGRect {
        CGRect(
            x: min(start.x, end.x),
            y: min(start.y, end.y),
            width: abs(end.x - start.x),
            height: abs(end.y - start.y)
        )
    }

    private static func makeMosaicImage(from image: CGImage) -> NSImage {
        let blockSize = max(10, image.width / 180)
        let width = max(1, image.width / blockSize)
        let height = max(1, image.height / blockSize)
        guard let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else {
            return NSImage(cgImage: image, size: NSSize(width: image.width, height: image.height))
        }
        context.interpolationQuality = .low
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        guard let reduced = context.makeImage() else {
            return NSImage(cgImage: image, size: NSSize(width: image.width, height: image.height))
        }
        return NSImage(
            cgImage: reduced,
            size: NSSize(width: image.width, height: image.height)
        )
    }
}
