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
        let featureColumns: Int
        let structuralFeatures: [UInt8]
    }

    private struct Similarity {
        let meanDifference: Double
        let matchRatio: Double
        let sampleCount: Int
        let rowCount: Int
    }

    private struct ShiftCandidate {
        let shift: Int
        let similarity: Similarity

        var score: Double {
            similarity.meanDifference + (1 - similarity.matchRatio) * 32
        }
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

        let unchanged = Self.sampledSimilarity(
            lhs: previous,
            lhsStartRow: 0,
            rhs: current,
            rhsStartRow: 0,
            rowCount: previous.height,
            sampleRows: 72,
            sampleColumns: 96
        )
        if unchanged.rowCount >= 10,
           unchanged.matchRatio >= 0.92,
           unchanged.meanDifference <= 4 {
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
            context.interpolationQuality = .none
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard created else { throw NativeLongScreenshotError.invalidImage }
        let features = structuralFeatures(pixels: pixels, width: width, height: height)
        return PixelFrame(
            width: width,
            height: height,
            pixels: pixels,
            featureColumns: features.columns,
            structuralFeatures: features.values
        )
    }

    private static func bestVerticalShift(
        previous: PixelFrame,
        current: PixelFrame
    ) -> Int? {
        let minimumShift = max(4, previous.height / 300)
        // A reliable stitch needs a substantial overlap. Faster scrolling is rejected so
        // repeated document sections cannot be mistaken for the adjacent viewport.
        let maximumShift = max(minimumShift, Int(Double(previous.height) * 0.72))
        let coarseStep = max(1, previous.height / 300)
        var coarseCandidates: [ShiftCandidate] = []
        var shift = minimumShift
        while shift <= maximumShift {
            let overlap = previous.height - shift
            let similarity = sampledSimilarity(
                lhs: previous,
                lhsStartRow: shift,
                rhs: current,
                rhsStartRow: 0,
                rowCount: overlap,
                sampleRows: min(96, overlap),
                sampleColumns: min(64, previous.featureColumns)
            )
            if similarity.sampleCount >= 40, similarity.rowCount >= 8 {
                coarseCandidates.append(ShiftCandidate(shift: shift, similarity: similarity))
            }
            shift += coarseStep
        }

        let seeds = coarseCandidates.sorted { $0.score < $1.score }.prefix(12)
        guard !seeds.isEmpty else { return nil }
        var candidates: [ShiftCandidate] = []
        var visitedShifts = Set<Int>()
        for seed in seeds {
            let start = max(minimumShift, seed.shift - coarseStep)
            let end = min(maximumShift, seed.shift + coarseStep)
            for refinedShift in start...end where visitedShifts.insert(refinedShift).inserted {
                let overlap = previous.height - refinedShift
                let similarity = sampledSimilarity(
                    lhs: previous,
                    lhsStartRow: refinedShift,
                    rhs: current,
                    rhsStartRow: 0,
                    rowCount: overlap,
                    sampleRows: min(180, overlap),
                    sampleColumns: min(80, previous.featureColumns)
                )
                if similarity.sampleCount >= 56, similarity.rowCount >= 10 {
                    candidates.append(ShiftCandidate(shift: refinedShift, similarity: similarity))
                }
            }
        }

        guard let best = candidates.min(by: { $0.score < $1.score }) else { return nil }
        let overlap = previous.height - best.shift
        let verification = sampledSimilarity(
            lhs: previous,
            lhsStartRow: best.shift,
            rhs: current,
            rhsStartRow: 0,
            rowCount: overlap,
            sampleRows: min(320, overlap),
            sampleColumns: previous.featureColumns
        )
        guard verification.sampleCount >= 72,
              verification.rowCount >= 12,
              verification.matchRatio >= 0.70,
              verification.meanDifference <= 50 else {
            return nil
        }

        // If a distant position is almost as convincing, the page contains repeated
        // structure and there is no safe way to know which copy is adjacent.
        let verifiedScore = ShiftCandidate(shift: best.shift, similarity: verification).score
        let ambiguous = candidates.contains { candidate in
            abs(candidate.shift - best.shift) > max(8, previous.height / 80)
                && candidate.similarity.matchRatio >= max(0.68, verification.matchRatio - 0.04)
                && candidate.score <= verifiedScore + 1.25
        }
        return ambiguous ? nil : best.shift
    }

    private static func sampledSimilarity(
        lhs: PixelFrame,
        lhsStartRow: Int,
        rhs: PixelFrame,
        rhsStartRow: Int,
        rowCount: Int,
        sampleRows: Int,
        sampleColumns: Int
    ) -> Similarity {
        guard rowCount > 0 else {
            return Similarity(
                meanDifference: .greatestFiniteMagnitude,
                matchRatio: 0,
                sampleCount: 0,
                rowCount: 0
            )
        }
        let rowInset = min(rowCount / 4, max(1, rowCount / 10))
        let usableRows = max(1, rowCount - rowInset * 2)
        var sampleCount = 0
        var rowDifferences: [Double] = []

        for rowSample in 0..<sampleRows {
            let relativeRow = rowInset + rowSample * max(0, usableRows - 1) / max(1, sampleRows - 1)
            let lhsRow = min(lhs.height - 1, lhsStartRow + relativeRow)
            let rhsRow = min(rhs.height - 1, rhsStartRow + relativeRow)
            var rowDifference = 0
            var rowSampleCount = 0
            for columnSample in 0..<sampleColumns {
                let featureColumn = columnSample
                    * max(0, lhs.featureColumns - 1) / max(1, sampleColumns - 1)
                let lhsFeature = Int(lhs.structuralFeatures[lhsRow * lhs.featureColumns + featureColumn])
                let rhsFeature = Int(rhs.structuralFeatures[rhsRow * rhs.featureColumns + featureColumn])
                guard lhsFeature > 10 || rhsFeature > 10 else {
                    continue
                }
                rowDifference += abs(lhsFeature - rhsFeature)
                rowSampleCount += 1
                sampleCount += 1
            }
            if rowSampleCount > 0 {
                rowDifferences.append(Double(rowDifference) / Double(rowSampleCount))
            }
        }
        guard sampleCount > 0, !rowDifferences.isEmpty else {
            return Similarity(
                meanDifference: .greatestFiniteMagnitude,
                matchRatio: 0,
                sampleCount: 0,
                rowCount: 0
            )
        }

        // Browser cursors, pets, sticky badges and loading indicators normally affect a
        // compact band of rows. Trimming the noisiest rows keeps those overlays from
        // turning an unchanged viewport into an apparent scroll.
        rowDifferences.sort()
        let retainedCount = max(1, Int((Double(rowDifferences.count) * 0.82).rounded(.down)))
        let retainedRows = rowDifferences.prefix(retainedCount)
        let meanDifference = retainedRows.reduce(0, +) / Double(retainedCount)
        let matchingRows = retainedRows.reduce(into: 0) { count, difference in
            if difference <= 14 { count += 1 }
        }
        return Similarity(
            meanDifference: meanDifference,
            matchRatio: Double(matchingRows) / Double(retainedCount),
            sampleCount: sampleCount,
            rowCount: retainedCount
        )
    }

    private static func structuralFeatures(
        pixels: [UInt8],
        width: Int,
        height: Int
    ) -> (columns: Int, values: [UInt8]) {
        let columns = min(96, max(24, width / 4))
        var values = [UInt8](repeating: 0, count: columns * height)
        for row in 0..<height {
            for column in 0..<columns {
                let x = column * max(0, width - 1) / max(1, columns - 1)
                let index = (row * width + x) * 4
                let red = Int(pixels[index])
                let green = Int(pixels[index + 1])
                let blue = Int(pixels[index + 2])
                let neighborPoints = [
                    (max(0, x - 2), row),
                    (min(width - 1, x + 2), row),
                    (x, max(0, row - 2)),
                    (x, min(height - 1, row + 2)),
                ]
                var strongestEdge = 0
                for (neighborX, neighborRow) in neighborPoints {
                    let neighborIndex = (neighborRow * width + neighborX) * 4
                    strongestEdge = max(
                        strongestEdge,
                        abs(red - Int(pixels[neighborIndex])),
                        abs(green - Int(pixels[neighborIndex + 1])),
                        abs(blue - Int(pixels[neighborIndex + 2]))
                    )
                }
                values[row * columns + column] = UInt8(min(255, strongestEdge))
            }
        }
        return (columns, values)
    }
}
