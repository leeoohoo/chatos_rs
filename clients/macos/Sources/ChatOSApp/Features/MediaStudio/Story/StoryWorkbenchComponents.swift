import AppKit
import ChatOSCore
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
            if let data = try? await MediaStudioImageLoader.data(for: asset), !Task.isCancelled { image = NSImage(data: data) }
        }
    }
}
