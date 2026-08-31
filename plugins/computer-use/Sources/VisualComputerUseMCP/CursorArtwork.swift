@preconcurrency import CoreGraphics
import Foundation

enum CursorArtwork {
    private static let trailLimit = 34

    static func drawTrail(
        _ rawPoints: [CGPoint],
        in context: CGContext,
        scale: Double = 1
    ) {
        let points = Array(rawPoints.suffix(trailLimit))
        guard points.count > 1 else { return }

        context.saveGState()
        context.setLineCap(.round)
        context.setLineJoin(.round)

        for index in 1..<points.count {
            let progress = Double(index) / Double(points.count - 1)
            let tailFade = progress * progress
            let previous = points[index - 1]
            let current = points[index]

            context.setStrokeColor(
                CGColor(
                    red: 0.36,
                    green: 0.40 + progress * 0.34,
                    blue: 1,
                    alpha: (0.02 + tailFade * 0.16)
                )
            )
            context.setLineWidth((4 + progress * 4) * scale)
            context.move(to: previous)
            context.addLine(to: current)
            context.strokePath()

            context.setStrokeColor(
                CGColor(
                    red: 0.48 - progress * 0.22,
                    green: 0.38 + progress * 0.43,
                    blue: 1,
                    alpha: 0.04 + tailFade * 0.68
                )
            )
            context.setLineWidth((1 + progress * 1.8) * scale)
            context.move(to: previous)
            context.addLine(to: current)
            context.strokePath()
        }
        context.restoreGState()
    }

    static func drawPointer(
        at point: CGPoint,
        size: Double,
        in context: CGContext
    ) {
        let colorSpace = CGColorSpace(name: CGColorSpace.sRGB)
        let coreRadius = max(8, size * 0.34)
        let orbitRadius = max(11, size * 0.46)
        let fogRadius = max(16, size * 0.70)

        if let colorSpace,
           let fog = CGGradient(
               colorsSpace: colorSpace,
               colors: [
                   CGColor(red: 0.30, green: 0.88, blue: 1, alpha: 0.25),
                   CGColor(red: 0.42, green: 0.30, blue: 1, alpha: 0.12),
                   CGColor(red: 0.18, green: 0.30, blue: 1, alpha: 0)
               ] as CFArray,
               locations: [0, 0.48, 1]
           ) {
            context.saveGState()
            context.drawRadialGradient(
                fog,
                startCenter: point,
                startRadius: 0,
                endCenter: point,
                endRadius: fogRadius,
                options: [.drawsAfterEndLocation]
            )
            context.restoreGState()
        }

        let coreRect = CGRect(
            x: point.x - coreRadius,
            y: point.y - coreRadius,
            width: coreRadius * 2,
            height: coreRadius * 2
        )
        context.saveGState()
        context.setShadow(
            offset: CGSize(width: 0, height: -size * 0.035),
            blur: size * 0.20,
            color: CGColor(red: 0, green: 0, blue: 0, alpha: 0.48)
        )
        context.setFillColor(CGColor(red: 0.025, green: 0.045, blue: 0.10, alpha: 0.94))
        context.fillEllipse(in: coreRect)
        context.restoreGState()

        if let colorSpace,
           let gradient = CGGradient(
               colorsSpace: colorSpace,
               colors: [
                   CGColor(red: 0.05, green: 0.08, blue: 0.18, alpha: 0.98),
                   CGColor(red: 0.16, green: 0.12, blue: 0.34, alpha: 0.96)
               ] as CFArray,
               locations: [0, 1]
           ) {
            context.saveGState()
            context.addEllipse(in: coreRect)
            context.clip()
            context.drawLinearGradient(
                gradient,
                start: CGPoint(x: coreRect.minX, y: coreRect.maxY),
                end: CGPoint(x: coreRect.maxX, y: coreRect.minY),
                options: []
            )
            context.restoreGState()
        }

        context.saveGState()
        context.setLineWidth(max(1, size * 0.038))
        context.setStrokeColor(CGColor(red: 0.86, green: 0.94, blue: 1, alpha: 0.72))
        context.strokeEllipse(in: coreRect)
        context.restoreGState()

        drawArc(
            center: point,
            radius: orbitRadius,
            startAngle: -.pi * 0.18,
            endAngle: .pi * 0.56,
            color: CGColor(red: 0.28, green: 0.93, blue: 1, alpha: 0.98),
            width: max(2, size * 0.075),
            in: context
        )
        drawArc(
            center: point,
            radius: orbitRadius,
            startAngle: .pi * 0.82,
            endAngle: .pi * 1.48,
            color: CGColor(red: 0.55, green: 0.38, blue: 1, alpha: 0.94),
            width: max(2, size * 0.075),
            in: context
        )

        context.saveGState()
        context.setLineCap(.round)
        context.setLineWidth(max(1.2, size * 0.045))
        context.setStrokeColor(CGColor(red: 0.90, green: 0.98, blue: 1, alpha: 0.92))
        let tickInner = coreRadius * 0.52
        let tickOuter = coreRadius * 0.86
        for angle in stride(from: 0.0, to: Double.pi * 2, by: Double.pi / 2) {
            context.move(
                to: CGPoint(
                    x: point.x + cos(angle) * tickInner,
                    y: point.y + sin(angle) * tickInner
                )
            )
            context.addLine(
                to: CGPoint(
                    x: point.x + cos(angle) * tickOuter,
                    y: point.y + sin(angle) * tickOuter
                )
            )
            context.strokePath()
        }
        context.restoreGState()

        let hotspotRadius = max(2.5, size * 0.082)
        context.saveGState()
        context.setFillColor(CGColor(red: 0.22, green: 0.92, blue: 1, alpha: 0.24))
        context.fillEllipse(
            in: CGRect(
                x: point.x - hotspotRadius * 2.2,
                y: point.y - hotspotRadius * 2.2,
                width: hotspotRadius * 4.4,
                height: hotspotRadius * 4.4
            )
        )
        context.setFillColor(CGColor(red: 0.22, green: 0.92, blue: 1, alpha: 1))
        context.fillEllipse(
            in: CGRect(
                x: point.x - hotspotRadius,
                y: point.y - hotspotRadius,
                width: hotspotRadius * 2,
                height: hotspotRadius * 2
            )
        )
        context.setLineWidth(max(1, size * 0.036))
        context.setStrokeColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.95))
        context.strokeEllipse(
            in: CGRect(
                x: point.x - hotspotRadius,
                y: point.y - hotspotRadius,
                width: hotspotRadius * 2,
                height: hotspotRadius * 2
            )
        )
        context.restoreGState()
    }

    private static func drawArc(
        center: CGPoint,
        radius: Double,
        startAngle: Double,
        endAngle: Double,
        color: CGColor,
        width: Double,
        in context: CGContext
    ) {
        context.saveGState()
        context.setLineCap(.round)
        context.setLineWidth(width)
        context.setStrokeColor(color)
        context.addArc(
            center: center,
            radius: radius,
            startAngle: startAngle,
            endAngle: endAngle,
            clockwise: false
        )
        context.strokePath()
        context.restoreGState()
    }

    static func drawOffscreenIndicator(
        at point: CGPoint,
        angle: Double,
        radius: Double,
        in context: CGContext
    ) {
        let rect = CGRect(
            x: point.x - radius,
            y: point.y - radius,
            width: radius * 2,
            height: radius * 2
        )
        context.saveGState()
        context.setShadow(
            offset: CGSize(width: 0, height: -2),
            blur: radius * 0.45,
            color: CGColor(red: 0, green: 0, blue: 0, alpha: 0.38)
        )
        context.setFillColor(CGColor(red: 0.04, green: 0.06, blue: 0.12, alpha: 0.82))
        context.fillEllipse(in: rect)
        context.restoreGState()

        context.saveGState()
        context.setLineWidth(max(1.8, radius * 0.14))
        context.setStrokeColor(CGColor(red: 0.42, green: 0.56, blue: 1, alpha: 0.95))
        context.strokeEllipse(in: rect)
        context.translateBy(x: point.x, y: point.y)
        context.rotate(by: angle)
        context.setLineCap(.round)
        context.setLineJoin(.round)
        context.setLineWidth(max(2, radius * 0.17))
        context.setStrokeColor(CGColor(red: 1, green: 1, blue: 1, alpha: 0.96))
        context.move(to: CGPoint(x: -radius * 0.30, y: radius * 0.48))
        context.addLine(to: CGPoint(x: radius * 0.38, y: 0))
        context.addLine(to: CGPoint(x: -radius * 0.30, y: -radius * 0.48))
        context.strokePath()
        context.restoreGState()
    }

}
