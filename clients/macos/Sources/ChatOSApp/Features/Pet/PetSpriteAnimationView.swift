import AppKit
import ChatOSCore
import ImageIO
import SwiftUI

struct PetSpriteAnimationView: View {
    private enum Atlas {
        static let cellWidth: CGFloat = 192
        static let cellHeight: CGFloat = 208
    }

    let animationState: PetAnimationState
    let isDragging: Bool
    let dragDirection: PetDragDirection
    let isAnimationActive: Bool

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var animationEpoch = Date()

    var body: some View {
        Group {
            if PetSpriteResource.isAvailable, isAnimationActive, !reduceMotion {
                TimelineView(.periodic(
                    from: animationEpoch,
                    by: frameDuration
                )) { context in
                    let frameIndex = currentFrameIndex(at: context.date)
                    if let frame = PetSpriteResource.frame(row: row, column: frameIndex) {
                        spriteFrame(frame)
                    } else {
                        fallbackCharacter
                    }
                }
            } else if let frame = PetSpriteResource.frame(row: row, column: 0) {
                spriteFrame(frame)
            } else {
                fallbackCharacter
            }
        }
        .onChange(of: animationState) { _, _ in
            animationEpoch = Date()
        }
        .onChange(of: isDragging) { _, _ in
            animationEpoch = Date()
        }
        .onChange(of: dragDirection) { _, _ in
            animationEpoch = Date()
        }
    }

    private func spriteFrame(_ frame: CGImage) -> some View {
        Image(decorative: frame, scale: 1)
            .resizable()
            .interpolation(.high)
            .antialiased(true)
            .aspectRatio(Atlas.cellWidth / Atlas.cellHeight, contentMode: .fit)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .shadow(color: .white.opacity(0.18), radius: 1)
            .shadow(
                color: .black.opacity(isDragging ? 0.12 : 0.28),
                radius: isDragging ? 2 : 5,
                y: isDragging ? 1 : 3
            )
    }

    private var row: Int {
        if isDragging {
            return dragDirection == .right ? 1 : 2
        }
        return switch animationState {
        case .idle: 0
        case .succeeded: 4
        case .failed: 5
        case .waiting: 6
        case .running: 7
        case .review: 8
        }
    }

    private var frameCount: Int {
        if isDragging {
            return 8
        }
        return switch animationState {
        case .idle: 7
        case .succeeded: 5
        case .failed: 8
        case .waiting, .running, .review: 6
        }
    }

    private var frameDuration: TimeInterval {
        if isDragging {
            return 0.10
        }
        return switch animationState {
        case .idle: 0.60
        case .succeeded: 0.13
        case .failed: 0.18
        case .waiting: 0.20
        case .running: 0.15
        case .review: 0.19
        }
    }

    private func currentFrameIndex(at date: Date) -> Int {
        let elapsed = max(0, date.timeIntervalSince(animationEpoch))
        return Int(elapsed / frameDuration) % frameCount
    }

    private var fallbackCharacter: some View {
        ZStack {
            Circle()
                .fill(Color(nsColor: .windowBackgroundColor))
                .overlay { Circle().stroke(.blue.opacity(0.34), lineWidth: 1.5) }
            Image(systemName: "face.smiling.inverse")
                .appFont(.system(size: 42, weight: .semibold))
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(.blue)
        }
    }
}

private enum PetSpriteResource {
    private static let columns = 8
    private static let rows = 11
    private static let cellWidth = 192
    private static let cellHeight = 208
    private static let renderedCellWidth = 180
    private static let renderedCellHeight = 195

    private static let frames: [CGImage]? = {
        let fileManager = FileManager.default
        let candidates: [URL?] = [
            Bundle.main.resourceURL?
                .appendingPathComponent("Pets", isDirectory: true)
                .appendingPathComponent("fengtuan", isDirectory: true)
                .appendingPathComponent("spritesheet.webp"),
            fileManager.homeDirectoryForCurrentUser
                .appendingPathComponent(".codex/pets/fengtuan/spritesheet.webp"),
        ]
        for case let url? in candidates where fileManager.fileExists(atPath: url.path) {
            guard let source = CGImageSourceCreateWithURL(url as CFURL, [
                kCGImageSourceShouldCache: false,
            ] as CFDictionary),
                  let atlas = CGImageSourceCreateImageAtIndex(source, 0, [
                    kCGImageSourceShouldCacheImmediately: true,
                  ] as CFDictionary),
                  atlas.width == columns * cellWidth,
                  atlas.height == rows * cellHeight else {
                continue
            }
            var renderedFrames: [CGImage] = []
            renderedFrames.reserveCapacity(rows * columns)
            for index in 0..<(rows * columns) {
                let row = index / columns
                let column = index % columns
                guard let cropped = atlas.cropping(to: CGRect(
                    x: column * cellWidth,
                    y: row * cellHeight,
                    width: cellWidth,
                    height: cellHeight
                )),
                    let context = CGContext(
                        data: nil,
                        width: renderedCellWidth,
                        height: renderedCellHeight,
                        bitsPerComponent: 8,
                        bytesPerRow: 0,
                        space: CGColorSpaceCreateDeviceRGB(),
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
                    ) else {
                    renderedFrames.removeAll()
                    break
                }
                context.interpolationQuality = .high
                context.draw(cropped, in: CGRect(
                    x: 0,
                    y: 0,
                    width: renderedCellWidth,
                    height: renderedCellHeight
                ))
                guard let rendered = context.makeImage() else {
                    renderedFrames.removeAll()
                    break
                }
                renderedFrames.append(rendered)
            }
            if renderedFrames.count == rows * columns {
                return renderedFrames
            }
        }
        return nil
    }()

    static var isAvailable: Bool {
        frames?.count == rows * columns
    }

    static func frame(row: Int, column: Int) -> CGImage? {
        guard let frames,
              (0..<rows).contains(row),
              (0..<columns).contains(column) else {
            return nil
        }
        return frames[row * columns + column]
    }
}
