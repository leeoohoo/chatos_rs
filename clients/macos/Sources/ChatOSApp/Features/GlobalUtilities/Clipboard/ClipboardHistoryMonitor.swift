import AppKit
import ChatOSCore
import Combine
import CryptoKit
import Foundation

@MainActor
final class ClipboardHistoryMonitor {
    static let restoredMarkerType = NSPasteboard.PasteboardType("com.chatos.clipboard-restored")

    var onEntryStored: ((ClipboardHistoryEntry) -> Void)?

    private let store: ClipboardHistoryStore
    private var monitorTask: Task<Void, Never>?
    private var lastChangeCount = NSPasteboard.general.changeCount
    private let maximumPayloadBytes = 25 * 1_024 * 1_024
    private var idlePollCount = 0
    private var isRequestedRunning = false
    private var isSystemAwake = true
    private var cancellables = Set<AnyCancellable>()

    init(store: ClipboardHistoryStore) {
        self.store = store
        let workspaceNotifications = NSWorkspace.shared.notificationCenter
        workspaceNotifications.publisher(for: NSWorkspace.willSleepNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.suspendForSystemSleep() }
            .store(in: &cancellables)
        workspaceNotifications.publisher(for: NSWorkspace.didWakeNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.resumeAfterSystemWake() }
            .store(in: &cancellables)
    }

    func start() {
        isRequestedRunning = true
        guard isSystemAwake else { return }
        guard monitorTask == nil else { return }
        lastChangeCount = NSPasteboard.general.changeCount
        idlePollCount = 0
        monitorTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                let delay = ClipboardPollingPolicy.interval(
                    isApplicationActive: NSApp.isActive,
                    idlePollCount: idlePollCount
                )
                try? await Task.sleep(for: delay)
                guard !Task.isCancelled else { return }
                if captureIfChanged() {
                    idlePollCount = 0
                } else {
                    idlePollCount = min(idlePollCount + 1, 32)
                }
            }
        }
    }

    func stop() {
        isRequestedRunning = false
        monitorTask?.cancel()
        monitorTask = nil
    }

    private func suspendForSystemSleep() {
        isSystemAwake = false
        monitorTask?.cancel()
        monitorTask = nil
    }

    private func resumeAfterSystemWake() {
        isSystemAwake = true
        if isRequestedRunning {
            start()
        }
    }

    @discardableResult
    private func captureIfChanged() -> Bool {
        let pasteboard = NSPasteboard.general
        guard pasteboard.changeCount != lastChangeCount else { return false }
        lastChangeCount = pasteboard.changeCount
        guard pasteboard.string(forType: Self.restoredMarkerType) == nil,
              !containsSensitiveType(pasteboard.types ?? []),
              let captured = capture(pasteboard) else {
            return true
        }

        let sourceBundleID = NSWorkspace.shared.frontmostApplication?.bundleIdentifier
        Task { [weak self, store] in
            do {
                let entry = try await store.add(
                    payload: captured.payload,
                    contentHash: captured.hash,
                    preview: captured.preview,
                    sourceBundleID: sourceBundleID
                )
                self?.onEntryStored?(entry)
            } catch {
                // Clipboard contents are private. Do not log payloads or previews here.
            }
        }
        return true
    }

    private func capture(_ pasteboard: NSPasteboard) -> CapturedClipboardPayload? {
        if let objects = pasteboard.readObjects(
            forClasses: [NSURL.self],
            options: [.urlReadingFileURLsOnly: true]
        ), !objects.isEmpty {
            let values = objects.compactMap { ($0 as? NSURL).map { $0 as URL } }
            guard values.count == objects.count else { return nil }
            let sortedPaths = values.map(\.standardizedFileURL.path).sorted()
            guard let data = try? JSONEncoder().encode(sortedPaths), data.count <= maximumPayloadBytes else {
                return nil
            }
            let preview = values.prefix(3).map(\.lastPathComponent).joined(separator: ", ")
            return CapturedClipboardPayload(
                payload: .files(values),
                hash: hash(prefix: "files", data: data),
                preview: preview
            )
        }

        let imageTypes: [NSPasteboard.PasteboardType] = [
            .png,
            NSPasteboard.PasteboardType("public.jpeg"),
            .tiff,
        ]
        for type in imageTypes {
            if let data = pasteboard.data(forType: type), data.count <= maximumPayloadBytes {
                return CapturedClipboardPayload(
                    payload: .image(data: data, pasteboardType: type.rawValue),
                    hash: hash(prefix: type.rawValue, data: data),
                    preview: nil
                )
            }
        }

        if let value = pasteboard.string(forType: .URL),
           let url = URL(string: value),
           let data = value.data(using: .utf8),
           data.count <= maximumPayloadBytes {
            return CapturedClipboardPayload(
                payload: .url(url),
                hash: hash(prefix: "url", data: data),
                preview: value
            )
        }

        if let value = pasteboard.string(forType: .string),
           let data = value.data(using: .utf8),
           !data.isEmpty,
           data.count <= maximumPayloadBytes {
            let preview = value
                .replacingOccurrences(of: "\n", with: " ")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return CapturedClipboardPayload(
                payload: .text(value),
                hash: hash(prefix: "text", data: data),
                preview: String(preview.prefix(360))
            )
        }
        return nil
    }

    private func containsSensitiveType(_ types: [NSPasteboard.PasteboardType]) -> Bool {
        let blockedFragments = [
            "org.nspasteboard.concealedtype",
            "org.nspasteboard.transienttype",
            "com.agilebits.onepassword",
            "com.1password",
            "com.lastpass",
            "com.bitwarden",
            "keepass",
        ]
        return types.contains { type in
            let value = type.rawValue.lowercased()
            return blockedFragments.contains(where: value.contains)
        }
    }

    private func hash(prefix: String, data: Data) -> String {
        var input = Data(prefix.utf8)
        input.append(0)
        input.append(data)
        return SHA256.hash(data: input).map { String(format: "%02x", $0) }.joined()
    }
}

enum ClipboardPollingPolicy {
    static func interval(
        isApplicationActive: Bool,
        idlePollCount: Int
    ) -> Duration {
        let boundedIdleCount = max(0, idlePollCount)
        if isApplicationActive {
            switch boundedIdleCount {
            case 0...1: return .milliseconds(300)
            case 2...4: return .milliseconds(600)
            case 5...9: return .seconds(1)
            default: return .milliseconds(1_500)
            }
        }
        switch boundedIdleCount {
        case 0...1: return .milliseconds(800)
        case 2...4: return .milliseconds(1_500)
        default: return .seconds(3)
        }
    }
}

private struct CapturedClipboardPayload {
    let payload: ClipboardHistoryPayload
    let hash: String
    let preview: String?
}
