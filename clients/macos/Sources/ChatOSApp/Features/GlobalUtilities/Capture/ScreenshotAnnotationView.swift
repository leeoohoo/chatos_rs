import AppKit
import CoreGraphics

enum ScreenshotAnnotationTool: Int {
    case pen
    case rectangle
}

private enum ScreenshotAnnotation {
    case pen(points: [CGPoint], color: NSColor, lineWidth: CGFloat)
    case rectangle(rect: CGRect, color: NSColor, lineWidth: CGFloat)
}

@MainActor
final class ScreenshotAnnotationView: NSView {
    var tool: ScreenshotAnnotationTool = .pen
    var annotationColor: NSColor = .systemRed
    var onAnnotationsChanged: (() -> Void)?

    private let sourceImage: CGImage
    private let displayImage: NSImage
    private var annotations: [ScreenshotAnnotation] = []
    private var workingAnnotation: ScreenshotAnnotation?
    private var dragOrigin: CGPoint?

    init(image: CGImage) {
        sourceImage = image
        displayImage = NSImage(
            cgImage: image,
            size: NSSize(width: image.width, height: image.height)
        )
        super.init(frame: .zero)
        wantsLayer = true
        layer?.backgroundColor = NSColor(calibratedWhite: 0.09, alpha: 1).cgColor
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    var canUndo: Bool {
        !annotations.isEmpty
    }

    var hasAnnotations: Bool {
        !annotations.isEmpty
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

        NSColor(calibratedWhite: 0.04, alpha: 1).setFill()
        NSBezierPath(roundedRect: frame.insetBy(dx: -1, dy: -1), xRadius: 5, yRadius: 5).fill()
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

        NSColor.white.withAlphaComponent(0.16).setStroke()
        let border = NSBezierPath(roundedRect: frame.insetBy(dx: -0.5, dy: -0.5), xRadius: 5, yRadius: 5)
        border.lineWidth = 1
        border.stroke()
    }

    override func mouseDown(with event: NSEvent) {
        let viewPoint = convert(event.locationInWindow, from: nil)
        guard imageFrame.contains(viewPoint) else { return }
        let point = imagePoint(from: viewPoint)
        dragOrigin = point
        switch tool {
        case .pen:
            workingAnnotation = .pen(
                points: [point],
                color: annotationColor,
                lineWidth: defaultLineWidth
            )
        case .rectangle:
            workingAnnotation = .rectangle(
                rect: CGRect(origin: point, size: .zero),
                color: annotationColor,
                lineWidth: defaultLineWidth
            )
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
        case let .rectangle(_, color, lineWidth):
            workingAnnotation = .rectangle(
                rect: normalizedRect(from: dragOrigin, to: point),
                color: color,
                lineWidth: lineWidth
            )
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
        case let .rectangle(rect, _, _):
            shouldKeep = rect.width >= 2 && rect.height >= 2
        }
        if shouldKeep, let completed = self.workingAnnotation {
            annotations.append(completed)
        }
        self.workingAnnotation = nil
        dragOrigin = nil
        needsDisplay = true
        onAnnotationsChanged?()
    }

    func undo() {
        guard !annotations.isEmpty else { return }
        annotations.removeLast()
        needsDisplay = true
        onAnnotationsChanged?()
    }

    func clear() {
        guard !annotations.isEmpty || workingAnnotation != nil else { return }
        annotations.removeAll()
        workingAnnotation = nil
        dragOrigin = nil
        needsDisplay = true
        onAnnotationsChanged?()
    }

    func renderedImage() -> CGImage? {
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

    private var imageFrame: NSRect {
        let available = bounds.insetBy(dx: 28, dy: 24)
        guard available.width > 0, available.height > 0 else { return .zero }
        let imageAspect = CGFloat(sourceImage.width) / CGFloat(sourceImage.height)
        let availableAspect = available.width / available.height
        if imageAspect > availableAspect {
            let height = available.width / imageAspect
            return NSRect(
                x: available.minX,
                y: available.midY - height / 2,
                width: available.width,
                height: height
            )
        }
        let width = available.height * imageAspect
        return NSRect(
            x: available.midX - width / 2,
            y: available.minY,
            width: width,
            height: available.height
        )
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
}
