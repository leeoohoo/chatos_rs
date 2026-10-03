import AppKit
import ChatOSCore
import Combine
import CryptoKit
import Foundation

@MainActor
final class ClipboardHistoryMonitor {
    static let restoredMarkerType = NSPasteboard.PasteboardType("com.chatos.clipboard-restored")

    var onEntryStored: ((ClipboardHistoryEntry) -> Void)?

    private let processor: ClipboardCaptureProcessor
    private var monitorTask: Task<Void, Never>?
    private var captureTask: Task<Void, Never>?
    private var captureGeneration: UUID?
    private var lastChangeCount = NSPasteboard.general.changeCount
    private let maximumPayloadBytes = 25 * 1_024 * 1_024
    private var idlePollCount = 0
    private var isRequestedRunning = false
    private var isSystemAwake = true
    private var cancellables = Set<AnyCancellable>()

    init(store: ClipboardHistoryStore) {
        self.processor = ClipboardCaptureProcessor(store: store)
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
        cancelCapture()
    }

    private func suspendForSystemSleep() {
        isSystemAwake = false
        monitorTask?.cancel()
        monitorTask = nil
        cancelCapture()
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
              let source = captureSource(pasteboard) else {
            return true
        }

        let sourceBundleID = NSWorkspace.shared.frontmostApplication?.bundleIdentifier
        let processor = processor
        let maximumPayloadBytes = maximumPayloadBytes
        let generation = UUID()
        captureTask?.cancel()
        captureGeneration = generation
        captureTask = Task { [weak self] in
            defer {
                if self?.captureGeneration == generation {
                    self?.captureTask = nil
                    self?.captureGeneration = nil
                }
            }
            do {
                try Task.checkCancellation()
                guard let entry = try await processor.store(
                    source,
                    sourceBundleID: sourceBundleID,
                    maximumPayloadBytes: maximumPayloadBytes
                ) else { return }
                try Task.checkCancellation()
                guard self?.captureGeneration == generation else { return }
                self?.onEntryStored?(entry)
            } catch {
                // Clipboard contents are private. Do not log payloads or previews here.
            }
        }
        return true
    }

    private func cancelCapture() {
        captureTask?.cancel()
        captureTask = nil
        captureGeneration = nil
    }

    private func captureSource(_ pasteboard: NSPasteboard) -> ClipboardCaptureSource? {
        if let objects = pasteboard.readObjects(
            forClasses: [NSURL.self],
            options: [.urlReadingFileURLsOnly: true]
        ), !objects.isEmpty {
            let values = objects.compactMap { ($0 as? NSURL).map { $0 as URL } }
            guard values.count == objects.count else { return nil }
            return .files(values)
        }

        let imageTypes: [NSPasteboard.PasteboardType] = [
            .png,
            NSPasteboard.PasteboardType("public.jpeg"),
            .tiff,
        ]
        for type in imageTypes {
            if let data = pasteboard.data(forType: type), data.count <= maximumPayloadBytes {
                return .image(data: data, pasteboardType: type.rawValue)
            }
        }

        if let value = pasteboard.string(forType: .URL) {
            guard !ClipboardSensitiveContentPolicy.shouldIgnore(value) else { return nil }
            return .url(value)
        }

        if let value = pasteboard.string(forType: .string) {
            guard !ClipboardSensitiveContentPolicy.shouldIgnore(value) else { return nil }
            return .text(value)
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
}

enum ClipboardSensitiveContentPolicy {
    private static let maximumInspectedCharacters = 128 * 1_024

    static func shouldIgnore(_ value: String) -> Bool {
        let sample = String(value.prefix(maximumInspectedCharacters)).lowercased()
        guard !sample.isEmpty else { return false }

        let privateKeyHeaders = [
            "-----begin private key-----",
            "-----begin rsa private key-----",
            "-----begin ec private key-----",
            "-----begin openssh private key-----",
        ]
        if privateKeyHeaders.contains(where: sample.contains) {
            return true
        }

        let hasCredentialWarning = sample.contains("\"warning_banner\"")
            && sample.contains("do not share")
            && (sample.contains("\"accesstoken\"") || sample.contains("\"access_token\""))
        if hasCredentialWarning {
            return true
        }

        return (sample.contains("\"accesstoken\"") && sample.contains("\"refreshtoken\""))
            || (sample.contains("\"access_token\"") && sample.contains("\"refresh_token\""))
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

enum ClipboardCaptureSource: Sendable {
    case files([URL])
    case image(data: Data, pasteboardType: String)
    case url(String)
    case text(String)
}

struct PreparedClipboardPayload: Sendable {
    let payload: ClipboardHistoryPayload
    let hash: String
    let preview: String?
}

enum ClipboardPayloadPreparation {
    static func prepare(
        _ source: ClipboardCaptureSource,
        maximumPayloadBytes: Int
    ) -> PreparedClipboardPayload? {
        switch source {
        case let .files(values):
            let sortedPaths = values.map(\.standardizedFileURL.path).sorted()
            guard let data = try? JSONEncoder().encode(sortedPaths),
                  data.count <= maximumPayloadBytes else {
                return nil
            }
            return PreparedClipboardPayload(
                payload: .files(values),
                hash: hash(prefix: "files", data: data),
                preview: values.prefix(3).map(\.lastPathComponent).joined(separator: ", ")
            )

        case let .image(data, pasteboardType):
            guard data.count <= maximumPayloadBytes else { return nil }
            return PreparedClipboardPayload(
                payload: .image(data: data, pasteboardType: pasteboardType),
                hash: hash(prefix: pasteboardType, data: data),
                preview: nil
            )

        case let .url(value):
            guard let url = URL(string: value),
                  let data = value.data(using: .utf8),
                  data.count <= maximumPayloadBytes else {
                return nil
            }
            return PreparedClipboardPayload(
                payload: .url(url),
                hash: hash(prefix: "url", data: data),
                preview: value
            )

        case let .text(value):
            guard let data = value.data(using: .utf8),
                  !data.isEmpty,
                  data.count <= maximumPayloadBytes else {
                return nil
            }
            let preview = value
                .replacingOccurrences(of: "\n", with: " ")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            return PreparedClipboardPayload(
                payload: .text(value),
                hash: hash(prefix: "text", data: data),
                preview: String(preview.prefix(360))
            )
        }
    }

    private static func hash(prefix: String, data: Data) -> String {
        var hasher = SHA256()
        hasher.update(data: Data(prefix.utf8))
        hasher.update(data: Data([0]))
        hasher.update(data: data)
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

private actor ClipboardCaptureProcessor {
    private let store: ClipboardHistoryStore

    init(store: ClipboardHistoryStore) {
        self.store = store
    }

    func store(
        _ source: ClipboardCaptureSource,
        sourceBundleID: String?,
        maximumPayloadBytes: Int
    ) async throws -> ClipboardHistoryEntry? {
        try Task.checkCancellation()
        guard let captured = ClipboardPayloadPreparation.prepare(
            source,
            maximumPayloadBytes: maximumPayloadBytes
        ) else {
            return nil
        }
        try Task.checkCancellation()
        return try await store.add(
            payload: captured.payload,
            contentHash: captured.hash,
            preview: captured.preview,
            sourceBundleID: sourceBundleID
        )
    }
}
