@preconcurrency import AppKit
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

    var body: some View {
        Group {
            if PetSpriteResource.isAvailable {
                NativePetSpriteView(configuration: PetSpriteAnimationPolicy.configuration(
                    animationState: animationState,
                    isDragging: isDragging,
                    dragDirection: dragDirection,
                    isAnimationActive: isAnimationActive,
                    reduceMotion: reduceMotion
                ))
            } else {
                fallbackCharacter
            }
        }
        .aspectRatio(Atlas.cellWidth / Atlas.cellHeight, contentMode: .fit)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
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

struct PetSpriteAnimationConfiguration: Equatable, Sendable {
    let row: Int
    let frameCount: Int
    let frameDuration: TimeInterval
    let shouldAnimate: Bool
}

enum PetSpriteAnimationPolicy {
    static func configuration(
        animationState: PetAnimationState,
        isDragging: Bool,
        dragDirection: PetDragDirection,
        isAnimationActive: Bool,
        reduceMotion: Bool
    ) -> PetSpriteAnimationConfiguration {
        let row: Int
        let frameCount: Int
        let frameDuration: TimeInterval

        if isDragging {
            row = dragDirection == .right ? 1 : 2
            frameCount = 8
            frameDuration = 0.10
        } else {
            switch animationState {
            case .idle:
                row = 0
                frameCount = 7
                frameDuration = 1.0
            case .succeeded:
                row = 4
                frameCount = 5
                frameDuration = 0.50
            case .failed:
                row = 5
                frameCount = 8
                frameDuration = 0.50
            case .waiting:
                row = 6
                frameCount = 6
                frameDuration = 0.50
            case .running:
                row = 7
                frameCount = 6
                frameDuration = 0.20
            case .review:
                row = 8
                frameCount = 6
                frameDuration = 0.25
            }
        }

        return PetSpriteAnimationConfiguration(
            row: row,
            frameCount: frameCount,
            frameDuration: frameDuration,
            shouldAnimate: isAnimationActive && !reduceMotion
        )
    }
}

/// Updates one layer's contents instead of invalidating a SwiftUI `TimelineView`
/// and its surrounding AttributeGraph on every sprite frame.
private struct NativePetSpriteView: NSViewRepresentable {
    let configuration: PetSpriteAnimationConfiguration

    func makeNSView(context: Context) -> PetSpriteLayerView {
        let view = PetSpriteLayerView()
        view.configure(configuration)
        return view
    }

    func updateNSView(_ nsView: PetSpriteLayerView, context: Context) {
        nsView.configure(configuration)
    }

    static func dismantleNSView(_ nsView: PetSpriteLayerView, coordinator: Void) {
        nsView.stopAnimating()
    }
}

@MainActor
private final class PetSpriteLayerView: NSView {
    private var configuration: PetSpriteAnimationConfiguration?
    private var animationTimer: Timer?
    private var frameIndex = 0

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.isOpaque = false
        layer?.backgroundColor = NSColor.clear.cgColor
        layer?.contentsGravity = .resizeAspect
        layer?.minificationFilter = .trilinear
        layer?.magnificationFilter = .linear
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var isOpaque: Bool { false }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard window != nil else {
            stopAnimating()
            return
        }
        restartTimerIfNeeded()
    }

    func configure(_ newConfiguration: PetSpriteAnimationConfiguration) {
        guard configuration != newConfiguration else { return }
        configuration = newConfiguration
        frameIndex = 0
        displayCurrentFrame()
        restartTimerIfNeeded()
    }

    func stopAnimating() {
        animationTimer?.invalidate()
        animationTimer = nil
    }

    private func restartTimerIfNeeded() {
        stopAnimating()
        guard window != nil,
              let configuration,
              configuration.shouldAnimate,
              configuration.frameCount > 1 else {
            return
        }

        let timer = Timer(timeInterval: configuration.frameDuration, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                self?.advanceFrame()
            }
        }
        timer.tolerance = min(0.10, configuration.frameDuration * 0.20)
        RunLoop.main.add(timer, forMode: .default)
        animationTimer = timer
    }

    private func advanceFrame() {
        guard let configuration else { return }
        frameIndex = (frameIndex + 1) % configuration.frameCount
        displayCurrentFrame()
    }

    private func displayCurrentFrame() {
        guard let configuration else { return }
        layer?.contents = PetSpriteResource.frame(
            row: configuration.row,
            column: frameIndex
        )
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
