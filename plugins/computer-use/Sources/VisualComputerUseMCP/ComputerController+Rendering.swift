@preconcurrency import AppKit
@preconcurrency import ApplicationServices
@preconcurrency import CoreGraphics
@preconcurrency import ImageIO
import Foundation
import UniformTypeIdentifiers

extension ComputerController {
    static func displayDTO(_ id: CGDirectDisplayID) -> DisplayDTO {
        let bounds = CGDisplayBounds(id)
        let pixelsWide = CGDisplayPixelsWide(id)
        let pixelsHigh = CGDisplayPixelsHigh(id)
        return DisplayDTO(
            id: id,
            isMain: CGDisplayIsMain(id) != 0,
            frame: RectDTO(
                x: bounds.origin.x,
                y: bounds.origin.y,
                width: bounds.width,
                height: bounds.height
            ),
            nativePixelWidth: pixelsWide,
            nativePixelHeight: pixelsHigh,
            nativePixelsPerPointX: Double(pixelsWide) / bounds.width,
            nativePixelsPerPointY: Double(pixelsHigh) / bounds.height
        )
    }

    static func contains(_ rect: RectDTO, _ point: PointDTO) -> Bool {
        point.x >= rect.x && point.x < rect.x + rect.width
            && point.y >= rect.y && point.y < rect.y + rect.height
    }

    static func contains(_ outer: RectDTO, _ inner: RectDTO) -> Bool {
        inner.width > 0 && inner.height > 0
            && inner.x >= outer.x
            && inner.y >= outer.y
            && inner.x + inner.width <= outer.x + outer.width
            && inner.y + inner.height <= outer.y + outer.height
    }

    static func screenshotPixel(
        for point: PointDTO,
        captureRegion: RectDTO,
        screenshotWidth: Int,
        screenshotHeight: Int
    ) -> PointDTO? {
        guard contains(captureRegion, point),
              screenshotWidth > 0,
              screenshotHeight > 0 else {
            return nil
        }
        return PointDTO(
            x: (point.x - captureRegion.x)
                * Double(screenshotWidth) / captureRegion.width,
            y: (point.y - captureRegion.y)
                * Double(screenshotHeight) / captureRegion.height
        )
    }

    static func desktopBounds(_ displays: [DisplayDTO]) -> RectDTO {
        guard let first = displays.first else {
            return RectDTO(x: 0, y: 0, width: 0, height: 0)
        }
        let minX = displays.reduce(first.frame.x) { min($0, $1.frame.x) }
        let minY = displays.reduce(first.frame.y) { min($0, $1.frame.y) }
        let maxX = displays.reduce(first.frame.x + first.frame.width) {
            max($0, $1.frame.x + $1.frame.width)
        }
        let maxY = displays.reduce(first.frame.y + first.frame.height) {
            max($0, $1.frame.y + $1.frame.height)
        }
        return RectDTO(
            x: minX,
            y: minY,
            width: maxX - minX,
            height: maxY - minY
        )
    }

    static func virtualTrajectory(
        from start: PointDTO,
        to target: PointDTO,
        steps: Int
    ) -> [PointDTO] {
        let count = max(2, min(steps, 80))
        let dx = target.x - start.x
        let dy = target.y - start.y
        let distance = hypot(dx, dy)
        guard distance > 0.5 else { return [start, target] }

        let normalX = -dy / distance
        let normalY = dx / distance
        let arc = min(90, distance * 0.13)
        return (0..<count).map { index in
            let t = Double(index) / Double(count - 1)
            let eased = t * t * (3 - 2 * t)
            let curve = sin(.pi * t) * arc
            return PointDTO(
                x: start.x + dx * eased + normalX * curve,
                y: start.y + dy * eased + normalY * curve
            )
        }
    }

    static func smoothScrollDeltas(total: Int32, steps: Int) -> [Int32] {
        let safeSteps = min(max(steps, 2), 80)
        var previous: Int64 = 0
        return (1...safeSteps).map { index in
            let progress = Double(index) / Double(safeSteps)
            let eased = progress * progress * (3 - 2 * progress)
            let cumulative = Int64((Double(total) * eased).rounded())
            defer { previous = cumulative }
            return Int32(cumulative - previous)
        }
    }

    static func render(
        image: CGImage,
        width: Int,
        height: Int,
        cursor: PointDTO,
        cursorTrail: [PointDTO],
        captureRegion: RectDTO,
        includeCursorMarker: Bool
    ) -> CGImage? {
        guard let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(
                data: nil,
                width: width,
                height: height,
                bitsPerComponent: 8,
                bytesPerRow: 0,
                space: colorSpace,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
              ) else {
            return nil
        }

        context.interpolationQuality = .high
        let scaleX = Double(width) / captureRegion.width
        let scaleY = Double(height) / captureRegion.height
        context.draw(
            image,
            in: CGRect(
                x: 0,
                y: 0,
                width: width,
                height: height
            )
        )

        if includeCursorMarker {
            let visibleTrail = cursorTrail.filter {
                contains(captureRegion, $0)
            }
            if visibleTrail.count > 1 {
                let points = visibleTrail.map { point in
                    CGPoint(
                        x: (point.x - captureRegion.x) * scaleX,
                        y: Double(height) - (point.y - captureRegion.y) * scaleY
                    )
                }
                CursorArtwork.drawTrail(points, in: context)
            }
        }

        if includeCursorMarker && contains(captureRegion, cursor) {
            let x = (cursor.x - captureRegion.x) * scaleX
            let yFromTop = (cursor.y - captureRegion.y) * scaleY
            let y = Double(height) - yFromTop
            let size = max(23.0, min(Double(width), Double(height)) * 0.032)
            CursorArtwork.drawPointer(
                at: CGPoint(x: x, y: y),
                size: size,
                in: context
            )
        } else if includeCursorMarker {
            let projectedX = (cursor.x - captureRegion.x) * scaleX
            let projectedY = Double(height)
                - (cursor.y - captureRegion.y) * scaleY
            let margin = max(
                24.0,
                min(Double(width), Double(height)) * 0.04
            )
            let indicator = CGPoint(
                x: min(max(projectedX, margin), Double(width) - margin),
                y: min(max(projectedY, margin), Double(height) - margin)
            )
            let directionX = projectedX - indicator.x
            let directionY = projectedY - indicator.y
            let angle = atan2(directionY, directionX)
            let radius = max(12.0, margin * 0.48)

            CursorArtwork.drawOffscreenIndicator(
                at: indicator,
                angle: angle,
                radius: radius,
                in: context
            )
        }
        return context.makeImage()
    }

    static func encodedData(
        from image: CGImage,
        format: ScreenshotFormat,
        jpegQuality: Double
    ) -> Data? {
        let quality = min(max(jpegQuality, 0.1), 1.0)
        switch format {
        case .png:
            return encodeUsingImageIO(
                image,
                type: UTType.png.identifier as CFString,
                properties: nil
            )
        case .jpeg:
            let properties = [
                kCGImageDestinationLossyCompressionQuality: quality
            ] as CFDictionary

            // ImageIO has occasionally finalized a malformed JPEG whose entropy
            // data contains an unescaped marker. Always validate the complete
            // byte stream before exposing it to an MCP client.
            for attempt in 1...2 {
                if let data = encodeUsingImageIO(
                    image,
                    type: UTType.jpeg.identifier as CFString,
                    properties: properties
                ), isValidJPEGEncoding(data) {
                    if attempt > 1 {
                        logEncodingRecovery(
                            "ImageIO JPEG encoding recovered on retry \(attempt)."
                        )
                    }
                    return data
                }
            }

            // Use a separate AppKit encoding entry point as the final recovery
            // path. Its output is subject to the same strict validation.
            let bitmap = NSBitmapImageRep(cgImage: image)
            if let data = bitmap.representation(
                using: .jpeg,
                properties: [.compressionFactor: quality]
            ), isValidJPEGEncoding(data) {
                logEncodingRecovery(
                    "ImageIO JPEG encoding failed validation; AppKit fallback succeeded."
                )
                return Data(data)
            }
            logEncodingRecovery(
                "JPEG encoding failed all validation and recovery attempts."
            )
            return nil
        }
    }

    private static func logEncodingRecovery(_ message: String) {
        let line = "visual-computer-use-mcp: \(message)\n"
        FileHandle.standardError.write(Data(line.utf8))
    }

    private static func encodeUsingImageIO(
        _ image: CGImage,
        type: CFString,
        properties: CFDictionary?
    ) -> Data? {
        guard let buffer = CFDataCreateMutable(kCFAllocatorDefault, 0) else {
            return nil
        }
        guard let destination = CGImageDestinationCreateWithData(
            buffer,
            type,
            1,
            nil
        ) else {
            return nil
        }
        CGImageDestinationAddImage(destination, image, properties)
        guard CGImageDestinationFinalize(destination) else { return nil }

        let count = CFDataGetLength(buffer)
        guard count > 0, let bytes = CFDataGetBytePtr(buffer) else {
            return nil
        }
        // Do not return storage shared with the mutable ImageIO destination.
        return Data(bytes: bytes, count: count)
    }

    static func isValidJPEGEncoding(_ data: Data) -> Bool {
        let bytes = [UInt8](data)
        guard bytes.count >= 4,
              bytes[0] == 0xFF,
              bytes[1] == 0xD8,
              bytes[bytes.count - 2] == 0xFF,
              bytes[bytes.count - 1] == 0xD9 else {
            return false
        }

        var index = 2
        while index < bytes.count - 2 {
            guard bytes[index] == 0xFF else { return false }

            while index < bytes.count, bytes[index] == 0xFF {
                index += 1
            }
            guard index < bytes.count else { return false }

            let marker = bytes[index]
            index += 1
            guard marker != 0x00,
                  marker != 0xD8,
                  marker != 0xD9,
                  !(0xD0...0xD7).contains(marker) else {
                return false
            }

            guard index + 1 < bytes.count else { return false }
            let segmentLength = Int(bytes[index]) << 8 | Int(bytes[index + 1])
            guard segmentLength >= 2 else { return false }
            let segmentEnd = index + segmentLength
            guard segmentEnd <= bytes.count else { return false }

            if marker == 0xDA {
                return hasValidJPEGEntropyData(bytes, startingAt: segmentEnd)
            }
            index = segmentEnd
        }
        return false
    }

    private static func hasValidJPEGEntropyData(
        _ bytes: [UInt8],
        startingAt start: Int
    ) -> Bool {
        var index = start
        while index < bytes.count {
            guard bytes[index] == 0xFF else {
                index += 1
                continue
            }

            var markerIndex = index + 1
            while markerIndex < bytes.count, bytes[markerIndex] == 0xFF {
                markerIndex += 1
            }
            guard markerIndex < bytes.count else { return false }

            let marker = bytes[markerIndex]
            if marker == 0x00 || (0xD0...0xD7).contains(marker) {
                index = markerIndex + 1
                continue
            }
            if marker == 0xD9 {
                return markerIndex == bytes.count - 1
            }

            // The ImageIO encoder used here emits one baseline scan. Any other
            // marker inside that scan means entropy data ended prematurely.
            return false
        }
        return false
    }

    static func applicationDTO(
        _ application: NSRunningApplication?
    ) -> ActiveApplicationDTO {
        ActiveApplicationDTO(
            name: application?.localizedName,
            bundleIdentifier: application?.bundleIdentifier,
            processIdentifier: application?.processIdentifier
        )
    }

    static func mouseButton(
        _ raw: String
    ) throws -> (down: CGEventType, up: CGEventType, button: CGMouseButton) {
        switch raw.lowercased() {
        case "left":
            return (.leftMouseDown, .leftMouseUp, .left)
        case "right":
            return (.rightMouseDown, .rightMouseUp, .right)
        case "middle", "center":
            return (.otherMouseDown, .otherMouseUp, .center)
        default:
            throw VisualComputerUseError.invalidArgument("button must be left, right, or middle.")
        }
    }
}
