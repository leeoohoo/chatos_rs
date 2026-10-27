import AppKit
import ChatOSCore
import SwiftUI

struct StorySurfaceModifier: ViewModifier {
    let tint: Color?
    func body(content: Content) -> some View {
        content
            .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
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
    @Environment(\.storyViewContext) private var appModel
    let asset: GeneratedMediaAsset?
    @State private var image: NSImage?
    var body: some View {
        ZStack {
            Color.primary.opacity(0.045)
            if let image { Image(nsImage: image).resizable().scaledToFit() }
            else { Image(systemName: "photo").foregroundStyle(.tertiary) }
        }.task(id: "\(appModel.authenticatedUserID ?? "signed-out")|\(asset?.id ?? "none")") {
            image = nil
            guard let asset,
                  let ownerUserID = appModel.authenticatedUserID else { return }
            let loaded = await GeneratedMediaAssetImageCache.image(
                for: asset,
                ownerUserID: ownerUserID,
                maximumDisplayPixelSize: 640
            )
            if !Task.isCancelled { image = loaded }
        }
    }
}
