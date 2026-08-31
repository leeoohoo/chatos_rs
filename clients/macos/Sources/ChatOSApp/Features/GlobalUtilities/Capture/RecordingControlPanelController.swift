import AppKit
import CoreGraphics
import SwiftUI

@MainActor
final class RecordingControlPanelController {
    var onStop: (() -> Void)?

    private var panel: NSPanel?
    private let status = RecordingControlStatus()

    var windowID: CGWindowID? {
        guard let panel, panel.windowNumber > 0 else { return nil }
        return CGWindowID(panel.windowNumber)
    }

    func prepare(isEnglish: Bool) {
        guard panel == nil else { return }
        let size = NSSize(width: 250, height: 54)
        let panel = NSPanel(
            contentRect: NSRect(origin: origin(for: size), size: size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.isReleasedWhenClosed = false
        panel.level = NSWindow.Level(rawValue: NSWindow.Level.screenSaver.rawValue + 2)
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        panel.hidesOnDeactivate = false
        panel.contentView = NSHostingView(rootView: RecordingControlView(
            status: status,
            isEnglish: isEnglish,
            onStop: { [weak self] in self?.onStop?() }
        ))
        self.panel = panel
    }

    func present(isEnglish: Bool) {
        prepare(isEnglish: isEnglish)
        guard let panel else { return }
        status.start()
        panel.orderFrontRegardless()
    }

    func dismiss() {
        status.stop()
        panel?.orderOut(nil)
        panel = nil
    }

    private func origin(for size: NSSize) -> NSPoint {
        let screen = NSScreen.screens.first(where: { NSMouseInRect(NSEvent.mouseLocation, $0.frame, false) })
            ?? NSScreen.main
            ?? NSScreen.screens[0]
        return NSPoint(
            x: screen.visibleFrame.maxX - size.width - 18,
            y: screen.visibleFrame.maxY - size.height - 18
        )
    }
}

@MainActor
private final class RecordingControlStatus: ObservableObject {
    @Published var elapsed: TimeInterval = 0
    private var task: Task<Void, Never>?
    private var startedAt = Date()

    func start() {
        startedAt = Date()
        elapsed = 0
        task?.cancel()
        task = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self else { return }
                elapsed = Date().timeIntervalSince(startedAt)
            }
        }
    }

    func stop() {
        task?.cancel()
        task = nil
    }
}

private struct RecordingControlView: View {
    @ObservedObject var status: RecordingControlStatus
    let isEnglish: Bool
    let onStop: () -> Void

    var body: some View {
        HStack(spacing: 11) {
            Circle().fill(.red).frame(width: 10, height: 10)
                .shadow(color: .red.opacity(0.5), radius: 4)
            Text(formattedElapsed)
                .font(.system(size: 14, weight: .semibold, design: .monospaced))
            Text(isEnglish ? "Recording" : "正在录制")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.secondary)
            Spacer()
            Button(action: onStop) {
                Image(systemName: "stop.fill")
                    .foregroundStyle(.white)
                    .frame(width: 28, height: 28)
                    .background(.red, in: Circle())
            }
            .buttonStyle(.plain)
            .help(isEnglish ? "Stop recording" : "停止录屏")
        }
        .padding(.horizontal, 14)
        .frame(width: 250, height: 54)
        .background(.ultraThickMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 14).strokeBorder(.white.opacity(0.16), lineWidth: 1)
        }
    }

    private var formattedElapsed: String {
        let seconds = Int(status.elapsed)
        return String(format: "%02d:%02d", seconds / 60, seconds % 60)
    }
}
