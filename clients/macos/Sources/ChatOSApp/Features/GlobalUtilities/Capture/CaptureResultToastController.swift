import AppKit
import SwiftUI

struct ScreenshotOutput {
    let image: CGImage
    let fileURL: URL?
    let copiedToPasteboard: Bool
    let errorMessage: String?
}

@MainActor
final class CaptureResultToastController: NSWindowController {
    private var dismissWorkItem: DispatchWorkItem?

    init() {
        let panel = NSPanel(
            contentRect: NSRect(origin: .zero, size: NSSize(width: 390, height: 126)),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.level = .floating
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .transient]
        super.init(window: panel)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func show(output: ScreenshotOutput, on screen: NSScreen, isEnglish: Bool) {
        dismissWorkItem?.cancel()
        guard let panel = window else { return }

        let view = CaptureResultToastView(
            output: output,
            isEnglish: isEnglish,
            onOpen: { [weak self] in
                guard let url = output.fileURL else { return }
                NSWorkspace.shared.open(url)
                self?.dismiss()
            },
            onReveal: { [weak self] in
                guard let url = output.fileURL else { return }
                NSWorkspace.shared.activateFileViewerSelecting([url])
                self?.dismiss()
            },
            onCopy: { [weak self] in
                ScreenshotCoordinator.copyToPasteboard(output.image)
                self?.dismiss()
            },
            onDismiss: { [weak self] in self?.dismiss() }
        )
        panel.contentView = NSHostingView(rootView: view)

        let visible = screen.visibleFrame
        panel.setFrameOrigin(NSPoint(
            x: visible.maxX - panel.frame.width - 22,
            y: visible.minY + 22
        ))
        panel.orderFrontRegardless()

        let workItem = DispatchWorkItem { [weak self] in self?.dismiss() }
        dismissWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 7, execute: workItem)
    }

    func dismiss() {
        dismissWorkItem?.cancel()
        dismissWorkItem = nil
        window?.orderOut(nil)
    }
}

private struct CaptureResultToastView: View {
    let output: ScreenshotOutput
    let isEnglish: Bool
    let onOpen: () -> Void
    let onReveal: () -> Void
    let onCopy: () -> Void
    let onDismiss: () -> Void

    var body: some View {
        HStack(spacing: 14) {
            Image(systemName: output.errorMessage == nil ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                .font(.system(size: 28, weight: .semibold))
                .foregroundStyle(output.errorMessage == nil ? Color.green : Color.orange)

            VStack(alignment: .leading, spacing: 7) {
                Text(title)
                    .font(.system(size: 14, weight: .semibold))
                Text(detail)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .lineLimit(2)

                HStack(spacing: 7) {
                    if output.fileURL != nil {
                        Button(isEnglish ? "Open" : "打开", action: onOpen)
                        Button(isEnglish ? "Show in Finder" : "在访达中显示", action: onReveal)
                    }
                    Button(isEnglish ? "Copy" : "复制", action: onCopy)
                }
                .controlSize(.small)
            }
            Spacer(minLength: 0)
            Button(action: onDismiss) {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
        .padding(16)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.ultraThickMaterial, in: RoundedRectangle(cornerRadius: 16))
        .overlay {
            RoundedRectangle(cornerRadius: 16)
                .strokeBorder(.white.opacity(0.16), lineWidth: 1)
        }
    }

    private var title: String {
        if output.fileURL != nil, output.copiedToPasteboard {
            return isEnglish ? "Screenshot saved and copied" : "截图已保存并复制"
        }
        if output.fileURL != nil {
            return isEnglish ? "Screenshot saved" : "截图已保存"
        }
        if output.copiedToPasteboard {
            return isEnglish ? "Screenshot copied" : "截图已复制"
        }
        return isEnglish ? "Screenshot failed" : "截图处理失败"
    }

    private var detail: String {
        output.errorMessage
            ?? output.fileURL?.lastPathComponent
            ?? (isEnglish ? "The image could not be written." : "图片未能写入。")
    }
}
