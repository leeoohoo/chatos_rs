import AppKit
import SwiftUI

@MainActor
final class RecordingResultToastController {
    private var panel: NSPanel?
    private var dismissTask: Task<Void, Never>?

    func present(url: URL, isEnglish: Bool) {
        dismiss()
        let size = NSSize(width: 390, height: 94)
        let screen = NSScreen.main ?? NSScreen.screens[0]
        let panel = NSPanel(
            contentRect: NSRect(
                x: screen.visibleFrame.maxX - size.width - 20,
                y: screen.visibleFrame.minY + 20,
                width: size.width,
                height: size.height
            ),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.level = .floating
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
        panel.contentView = NSHostingView(rootView: RecordingResultToastView(
            url: url,
            isEnglish: isEnglish,
            onOpen: { NSWorkspace.shared.open(url) },
            onReveal: { NSWorkspace.shared.activateFileViewerSelecting([url]) }
        ))
        self.panel = panel
        panel.orderFrontRegardless()
        dismissTask = Task { [weak self] in
            try? await Task.sleep(for: .seconds(8))
            self?.dismiss()
        }
    }

    private func dismiss() {
        dismissTask?.cancel()
        dismissTask = nil
        panel?.orderOut(nil)
        panel = nil
    }
}

private struct RecordingResultToastView: View {
    let url: URL
    let isEnglish: Bool
    let onOpen: () -> Void
    let onReveal: () -> Void

    var body: some View {
        HStack(spacing: 13) {
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 28))
                .foregroundStyle(.green)
            VStack(alignment: .leading, spacing: 3) {
                Text(isEnglish ? "Recording saved" : "录屏已保存")
                    .font(.system(size: 14, weight: .semibold))
                Text(url.lastPathComponent)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            Button(isEnglish ? "Open" : "打开", action: onOpen)
            Button(isEnglish ? "Show" : "显示", action: onReveal)
        }
        .padding(15)
        .frame(width: 390, height: 94)
        .background(.ultraThickMaterial, in: RoundedRectangle(cornerRadius: 15))
        .overlay { RoundedRectangle(cornerRadius: 15).strokeBorder(.white.opacity(0.16)) }
    }
}
