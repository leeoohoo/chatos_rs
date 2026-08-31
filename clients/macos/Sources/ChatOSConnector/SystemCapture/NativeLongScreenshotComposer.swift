import CoreGraphics
import Foundation

public enum NativeLongScreenshotAppendResult: Equatable, Sendable {
    case appended(newRows: Int, totalHeight: Int)
    case unchanged(totalHeight: Int)
    case overlapNotFound(totalHeight: Int)
}

public enum NativeLongScreenshotError: LocalizedError {
    case invalidImage
    case sizeChanged
    case notStarted
    case outputTooLarge

    public var errorDescription: String? {
        switch self {
        case .invalidImage:
            "The screenshot frame could not be decoded."
        case .sizeChanged:
            "The selected screenshot region changed size."
        case .notStarted:
            "The long screenshot session has not started."
        case .outputTooLarge:
            "The long screenshot reached its safe output limit."
        }
    }
}

public actor NativeLongScreenshotComposer {
    private struct PixelFrame {
        let width: Int
        let height: Int
        let pixels: [UInt8]
    }

    private let maximumPixelCount: Int
    private var width = 0
    private var frameHeight = 0
    private var stitchedPixels: [UInt8] = []
    private var lastFrame: PixelFrame?

    public init(maximumPixelCount: Int = 150_000_000) {
        self.maximumPixelCount = maximumPixelCount
    }

    public func start(with image: CGImage) throws {
        let frame = try Self.decode(image)
        guard frame.width * frame.height <= maximumPixelCount else {
            throw NativeLongScreenshotError.outputTooLarge
        }
        width = frame.width
        frameHeight = frame.height
        stitchedPixels = frame.pixels
        lastFrame = frame
    }

    public func append(_ image: CGImage) throws -> NativeLongScreenshotAppendResult {
        guard let previous = lastFrame else {
            throw NativeLongScreenshotError.notStarted
        }
        let current = try Self.decode(image)
        guard current.width == width, current.height == frameHeight else {
            throw NativeLongScreenshotError.sizeChanged
        }

        if Self.meanDifference(previous, current) < 2.2 {
            return .unchanged(totalHeight: stitchedHeight)
        }
        guard let shift = Self.bestVerticalShift(previous: previous, current: current) else {
            return .overlapNotFound(totalHeight: stitchedHeight)
        }
        guard width * (stitchedHeight + shift) <= maximumPixelCount else {
            throw NativeLongScreenshotError.outputTooLarge
        }

        let startRow = current.height - shift
        let startByte = startRow * current.width * 4
        stitchedPixels.append(contentsOf: current.pixels[startByte...])
        lastFrame = current
        return .appended(newRows: shift, totalHeight: stitchedHeight)
    }

    public func outputImage() throws -> CGImage {
        guard width > 0, stitchedHeight > 0, !stitchedPixels.isEmpty else {
            throw NativeLongScreenshotError.notStarted
        }
        let data = Data(stitchedPixels) as CFData
        guard let provider = CGDataProvider(data: data),
              let image = CGImage(
                width: width,
                height: stitchedHeight,
                bitsPerComponent: 8,
                bitsPerPixel: 32,
                bytesPerRow: width * 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                provider: provider,
                decode: nil,
                shouldInterpolate: true,
                intent: .defaultIntent
              ) else {
            throw NativeLongScreenshotError.invalidImage
        }
        return image
    }

    public var currentHeight: Int {
        stitchedHeight
    }

    private var stitchedHeight: Int {
        width > 0 ? stitchedPixels.count / (width * 4) : 0
    }

    private static func decode(_ image: CGImage) throws -> PixelFrame {
        let width = image.width
        let height = image.height
        guard width > 0, height > 0 else {
            throw NativeLongScreenshotError.invalidImage
        }
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let created = pixels.withUnsafeMutableBytes { bytes -> Bool in
            guard let baseAddress = bytes.baseAddress,
                  let context = CGContext(
                    data: baseAddress,
                    width: width,
                    height: height,
                    bitsPerComponent: 8,
                    bytesPerRow: width * 4,
                    space: CGColorSpaceCreateDeviceRGB(),
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
                  ) else { return false }
            context.translateBy(x: 0, y: CGFloat(height))
            context.scaleBy(x: 1, y: -1)
            context.interpolationQuality = .none
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard created else { throw NativeLongScreenshotError.invalidImage }
        return PixelFrame(width: width, height: height, pixels: pixels)
    }

    private static func meanDifference(_ lhs: PixelFrame, _ rhs: PixelFrame) -> Double {
        sampledDifference(
            lhs: lhs,
            lhsStartRow: 0,
            rhs: rhs,
            rhsStartRow: 0,
            rowCount: lhs.height,
            sampleRows: 36,
            sampleColumns: 48
        )
    }

    private static func bestVerticalShift(
        previous: PixelFrame,
        current: PixelFrame
    ) -> Int? {
        let minimumShift = max(4, previous.height / 300)
        let maximumShift = max(minimumShift, Int(Double(previous.height) * 0.82))
        let coarseStep = max(1, previous.height / 420)
        var bestShift: Int?
        var bestScore = Double.greatestFiniteMagnitude

        var shift = minimumShift
        while shift <= maximumShift {
            let overlap = previous.height - shift
            let score = sampledDifference(
                lhs: previous,
                lhsStartRow: shift,
                rhs: current,
                rhsStartRow: 0,
                rowCount: overlap,
                sampleRows: 54,
                sampleColumns: 64
            )
            if score < bestScore {
                bestScore = score
                bestShift = shift
            }
            shift += coarseStep
        }

        guard let coarseBest = bestShift else { return nil }
        let refinementStart = max(minimumShift, coarseBest - coarseStep)
        let refinementEnd = min(maximumShift, coarseBest + coarseStep)
        for refinedShift in refinementStart...refinementEnd {
            let overlap = previous.height - refinedShift
            let score = sampledDifference(
                lhs: previous,
                lhsStartRow: refinedShift,
                rhs: current,
                rhsStartRow: 0,
                rowCount: overlap,
                sampleRows: 72,
                sampleColumns: 80
            )
            if score < bestScore {
                bestScore = score
                bestShift = refinedShift
            }
        }

        return bestScore <= 24 ? bestShift : nil
    }

    private static func sampledDifference(
        lhs: PixelFrame,
        lhsStartRow: Int,
        rhs: PixelFrame,
        rhsStartRow: Int,
        rowCount: Int,
        sampleRows: Int,
        sampleColumns: Int
    ) -> Double {
        guard rowCount > 0 else { return .greatestFiniteMagnitude }
        let rowInset = min(rowCount / 5, max(1, rowCount / 20))
        let usableRows = max(1, rowCount - rowInset * 2)
        var difference: Int64 = 0
        var sampleCount: Int64 = 0

        for rowSample in 0..<sampleRows {
            let relativeRow = rowInset + rowSample * max(0, usableRows - 1) / max(1, sampleRows - 1)
            let lhsRow = min(lhs.height - 1, lhsStartRow + relativeRow)
            let rhsRow = min(rhs.height - 1, rhsStartRow + relativeRow)
            for columnSample in 0..<sampleColumns {
                let x = columnSample * max(0, lhs.width - 1) / max(1, sampleColumns - 1)
                let lhsIndex = (lhsRow * lhs.width + x) * 4
                let rhsIndex = (rhsRow * rhs.width + x) * 4
                difference += Int64(abs(Int(lhs.pixels[lhsIndex]) - Int(rhs.pixels[rhsIndex])))
                difference += Int64(abs(Int(lhs.pixels[lhsIndex + 1]) - Int(rhs.pixels[rhsIndex + 1])))
                difference += Int64(abs(Int(lhs.pixels[lhsIndex + 2]) - Int(rhs.pixels[rhsIndex + 2])))
                sampleCount += 3
            }
        }
        return sampleCount > 0 ? Double(difference) / Double(sampleCount) : .greatestFiniteMagnitude
    }
}
