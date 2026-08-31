import AppKit
import ChatOSCore
import SwiftUI

struct HotKeyRecorderView: View {
    var hotKey: GlobalHotKey
    var isEnglish: Bool
    var onChange: (GlobalHotKey) -> Void

    @State private var isRecording = false
    @State private var monitor: Any?

    var body: some View {
        Button {
            isRecording ? stopRecording() : startRecording()
        } label: {
            HStack(spacing: 7) {
                Image(systemName: isRecording ? "keyboard.badge.ellipsis" : "keyboard")
                Text(isRecording
                    ? (isEnglish ? "Press shortcut…" : "请按下快捷键…")
                    : hotKey.displayName)
                    .appFont(.callout.monospaced())
            }
            .frame(minWidth: 124)
        }
        .buttonStyle(.bordered)
        .onDisappear(perform: stopRecording)
    }

    private func startRecording() {
        stopRecording()
        isRecording = true
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            if event.keyCode == 53 {
                Task { @MainActor in stopRecording() }
                return nil
            }
            guard let candidate = GlobalHotKey(event: event) else { return nil }
            Task { @MainActor in
                onChange(candidate)
                stopRecording()
            }
            return nil
        }
    }

    private func stopRecording() {
        if let monitor {
            NSEvent.removeMonitor(monitor)
            self.monitor = nil
        }
        isRecording = false
    }
}

private extension GlobalHotKey {
    init?(event: NSEvent) {
        let flags = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        var modifiers: GlobalHotKeyModifiers = []
        if flags.contains(.command) { modifiers.insert(.command) }
        if flags.contains(.option) { modifiers.insert(.option) }
        if flags.contains(.control) { modifiers.insert(.control) }
        if flags.contains(.shift) { modifiers.insert(.shift) }
        guard !modifiers.isEmpty else { return nil }

        let keyEquivalent: String
        switch event.keyCode {
        case 36: keyEquivalent = "Return"
        case 48: keyEquivalent = "Tab"
        case 49: keyEquivalent = "Space"
        case 51: keyEquivalent = "Delete"
        case 53: keyEquivalent = "Escape"
        case 123: keyEquivalent = "←"
        case 124: keyEquivalent = "→"
        case 125: keyEquivalent = "↓"
        case 126: keyEquivalent = "↑"
        default:
            let value = event.charactersIgnoringModifiers?
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .uppercased() ?? ""
            guard !value.isEmpty else { return nil }
            keyEquivalent = value
        }

        self.init(
            keyCode: UInt32(event.keyCode),
            keyEquivalent: keyEquivalent,
            modifiers: modifiers
        )
    }
}
