import AppKit
import ChatOSCore
import ImageIO
import SwiftUI

struct StorySurfaceModifier: ViewModifier {
    let tint: Color?
    func body(content: Content) -> some View {
        content
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .strokeBorder(
                        LinearGradient(colors: [
                            (tint ?? .primary).opacity(tint == nil ? 0.09 : 0.18),
                            Color.primary.opacity(0.045),
                        ], startPoint: .topLeading, endPoint: .bottomTrailing)
                    )
            }
            .shadow(color: Color.black.opacity(0.035), radius: 12, y: 4)
    }
}

extension View {
    func storySurface(tint: Color? = nil) -> some View {
        modifier(StorySurfaceModifier(tint: tint))
    }
}

struct StoryThumbnail: View {
    let asset: GeneratedMediaAsset?
    @State private var image: NSImage?
    var body: some View {
        ZStack {
            Color.primary.opacity(0.045)
            if let image { Image(nsImage: image).resizable().scaledToFit() }
            else { Image(systemName: "photo").foregroundStyle(.tertiary) }
        }.task(id: asset?.id) {
            image = nil
            guard let asset else { return }
            let loaded = await StoryThumbnailCache.image(for: asset)
            if !Task.isCancelled { image = loaded }
        }
    }
}

@MainActor
private enum StoryThumbnailCache {
    private static let maximumPixelSize = 640
    private static let cache: NSCache<NSString, NSImage> = {
        let cache = NSCache<NSString, NSImage>()
        cache.countLimit = 96
        cache.totalCostLimit = 96 * 1_024 * 1_024
        return cache
    }()
    private static var inFlight: [String: Task<CGImage?, Never>] = [:]

    static func image(for asset: GeneratedMediaAsset) async -> NSImage? {
        let key = cacheKey(for: asset)
        if let cached = cache.object(forKey: key as NSString) {
            return cached
        }

        let task: Task<CGImage?, Never>
        if let existing = inFlight[key] {
            task = existing
        } else {
            task = Task.detached(priority: .userInitiated) {
                guard let data = try? await MediaStudioImageLoader.data(for: asset) else {
                    return nil
                }
                return thumbnail(from: data)
            }
            inFlight[key] = task
        }

        let cgImage = await task.value
        inFlight.removeValue(forKey: key)
        guard let cgImage else { return nil }
        let image = NSImage(cgImage: cgImage, size: .zero)
        cache.setObject(
            image,
            forKey: key as NSString,
            cost: cgImage.width * cgImage.height * 4
        )
        return image
    }

    private static func cacheKey(for asset: GeneratedMediaAsset) -> String {
        "\(asset.id)|\(asset.url?.absoluteString ?? "inline")|\(maximumPixelSize)"
    }

    nonisolated private static func thumbnail(from data: Data) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, [
            kCGImageSourceShouldCache: false,
        ] as CFDictionary) else { return nil }
        return CGImageSourceCreateThumbnailAtIndex(source, 0, [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: 640,
            kCGImageSourceShouldCacheImmediately: true,
        ] as CFDictionary)
    }
}
