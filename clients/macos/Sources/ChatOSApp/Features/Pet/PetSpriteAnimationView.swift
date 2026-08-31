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

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var animationEpoch = Date()

    var body: some View {
        Group {
            if PetSpriteResource.isAvailable {
                TimelineView(.animation(
                    minimumInterval: frameDuration,
                    paused: reduceMotion
                )) { context in
                    let frameIndex = reduceMotion
                        ? 0
                        : currentFrameIndex(at: context.date)
                    if let frame = PetSpriteResource.frame(row: row, column: frameIndex) {
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
                    } else {
                        fallbackCharacter
                    }
                }
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
        case .idle: 0.22
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

    private static let spritesheet: CGImage? = {
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
            guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
                  let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
                  image.width == columns * cellWidth,
                  image.height == rows * cellHeight else {
                continue
            }
            if image.bitsPerPixel > 0 {
                return image
            }
        }
        return nil
    }()

    private static let frames: [CGImage]? = {
        guard let spritesheet else { return nil }
        return (0..<(rows * columns)).compactMap { index in
            let row = index / columns
            let column = index % columns
            return spritesheet.cropping(to: CGRect(
                x: column * cellWidth,
                y: row * cellHeight,
                width: cellWidth,
                height: cellHeight
            ))
        }
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
