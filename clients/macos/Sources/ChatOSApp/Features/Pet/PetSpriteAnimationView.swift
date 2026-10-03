@preconcurrency import AppKit
import ChatOSCore
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
    @State private var spriteIsReady = false

    var body: some View {
        Group {
            if spriteIsReady || PetSpriteResource.isAvailable {
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
        .task {
            spriteIsReady = await PetSpriteResource.prepare()
        }
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

enum PetSpriteAtlasPolicy {
    static let columns = 8
    static let rows = 11
    static let cellWidth = 192
    static let cellHeight = 208
    static let renderedCellWidth = 180
    static let renderedCellHeight = 195
    static let maximumBytes = 20 * 1_024 * 1_024
    static let sourcePixelCount = columns * cellWidth * rows * cellHeight

    static func hasExpectedDimensions(width: Int, height: Int) -> Bool {
        width == columns * cellWidth && height == rows * cellHeight
    }
}

@MainActor
enum PetSpriteResource {
    private static var frames: [CGImage]?
    private static let taskPool = AppSharedTaskPool<[CGImage]>(priority: .utility)

    static func prepare() async -> Bool {
        if isAvailable { return true }
        let loaded = await taskPool.value(for: "fengtuan-sprite-atlas") { loadFrames() }
        guard !Task.isCancelled else { return false }
        if frames == nil { frames = loaded }
        return isAvailable
    }

    nonisolated private static func loadFrames() -> [CGImage]? {
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
            guard !Task.isCancelled,
                  let data = try? AppBoundedFileReader.read(
                    url,
                    maximumBytes: PetSpriteAtlasPolicy.maximumBytes
                  ),
                  let decoded = AppImageThumbnailLoader.decode(
                    data,
                    maximumSourcePixelCount: PetSpriteAtlasPolicy.sourcePixelCount,
                    maximumDisplayPixelSize: max(
                        PetSpriteAtlasPolicy.columns * PetSpriteAtlasPolicy.cellWidth,
                        PetSpriteAtlasPolicy.rows * PetSpriteAtlasPolicy.cellHeight
                    )
                  ),
                  PetSpriteAtlasPolicy.hasExpectedDimensions(
                    width: decoded.image.width,
                    height: decoded.image.height
                  ) else {
                continue
            }
            let atlas = decoded.image
            var renderedFrames: [CGImage] = []
            renderedFrames.reserveCapacity(
                PetSpriteAtlasPolicy.rows * PetSpriteAtlasPolicy.columns
            )
            for index in 0..<(PetSpriteAtlasPolicy.rows * PetSpriteAtlasPolicy.columns) {
                guard !Task.isCancelled else { return nil }
                let row = index / PetSpriteAtlasPolicy.columns
                let column = index % PetSpriteAtlasPolicy.columns
                guard let cropped = atlas.cropping(to: CGRect(
                    x: column * PetSpriteAtlasPolicy.cellWidth,
                    y: row * PetSpriteAtlasPolicy.cellHeight,
                    width: PetSpriteAtlasPolicy.cellWidth,
                    height: PetSpriteAtlasPolicy.cellHeight
                )),
                    let context = CGContext(
                        data: nil,
                        width: PetSpriteAtlasPolicy.renderedCellWidth,
                        height: PetSpriteAtlasPolicy.renderedCellHeight,
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
                    width: PetSpriteAtlasPolicy.renderedCellWidth,
                    height: PetSpriteAtlasPolicy.renderedCellHeight
                ))
                guard let rendered = context.makeImage() else {
                    renderedFrames.removeAll()
                    break
                }
                renderedFrames.append(rendered)
            }
            if renderedFrames.count == PetSpriteAtlasPolicy.rows * PetSpriteAtlasPolicy.columns {
                return renderedFrames
            }
        }
        return nil
    }

    static var isAvailable: Bool {
        frames?.count == PetSpriteAtlasPolicy.rows * PetSpriteAtlasPolicy.columns
    }

    static func frame(row: Int, column: Int) -> CGImage? {
        guard let frames,
              (0..<PetSpriteAtlasPolicy.rows).contains(row),
              (0..<PetSpriteAtlasPolicy.columns).contains(column) else {
            return nil
        }
        return frames[row * PetSpriteAtlasPolicy.columns + column]
    }
}
