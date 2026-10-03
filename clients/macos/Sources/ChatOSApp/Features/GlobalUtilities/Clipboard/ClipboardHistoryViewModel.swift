import AppKit
import ChatOSCore
import Foundation
import SwiftUI

private enum ClipboardHistoryPasteError: LocalizedError {
    case writeFailed

    var errorDescription: String? {
        "无法把所选内容写入系统剪贴板。"
    }
}

enum ClipboardThumbnailPolicy {
    static let maximumSourcePixelCount = 64_000_000
    static let maximumDisplayPixelSize = 180
    static let maximumCachedCount = 64
}

@MainActor
final class ClipboardHistoryViewModel: ObservableObject {
    @Published var query = ""
    @Published private(set) var entries: [ClipboardHistoryEntry] = []
    @Published private(set) var selectedIndex = 0
    @Published private(set) var isLoading = false
    @Published private(set) var errorMessage: String?
    @Published private(set) var noticeMessage: String?
    @Published private(set) var imageThumbnails: [UUID: NSImage] = [:]

    var onRestoreSucceeded: (() -> Void)?
    var onCancel: (() -> Void)?

    private struct ThumbnailLoad {
        let id: UUID
        let task: Task<Void, Never>
    }

    private let store: ClipboardHistoryStore
    private var refreshTask: Task<Void, Never>?
    private var refreshGeneration: UUID?
    private var thumbnailTasks: [UUID: ThumbnailLoad] = [:]
    private var thumbnailRecency: [UUID] = []

    init(store: ClipboardHistoryStore) {
        self.store = store
    }

    deinit {
        refreshTask?.cancel()
        thumbnailTasks.values.forEach { $0.task.cancel() }
    }

    var filteredEntries: [ClipboardHistoryEntry] {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return entries }
        return entries.filter { entry in
            entry.textPreview?.localizedCaseInsensitiveContains(trimmed) == true
                || entry.sourceApplicationBundleID?.localizedCaseInsensitiveContains(trimmed) == true
                || entry.kind.rawValue.localizedCaseInsensitiveContains(trimmed)
        }
    }

    func prepareForPresentation() {
        query = ""
        selectedIndex = 0
        errorMessage = nil
        noticeMessage = nil
        refresh()
    }

    func refresh() {
        refreshTask?.cancel()
        let generation = UUID()
        refreshGeneration = generation
        isLoading = true
        refreshTask = Task { [weak self, store] in
            defer {
                if self?.refreshGeneration == generation {
                    self?.refreshTask = nil
                    self?.refreshGeneration = nil
                    self?.isLoading = false
                }
            }
            do {
                let values = try await store.entries()
                try Task.checkCancellation()
                guard self?.refreshGeneration == generation else { return }
                self?.entries = values
                self?.pruneThumbnailCache(validEntries: values)
                self?.selectedIndex = min(self?.selectedIndex ?? 0, max(0, values.count - 1))
                self?.errorMessage = nil
            } catch is CancellationError {
                return
            } catch {
                guard self?.refreshGeneration == generation else { return }
                self?.errorMessage = error.localizedDescription
            }
        }
    }

    func entryWasStored(_ entry: ClipboardHistoryEntry) {
        entries.removeAll { $0.id == entry.id }
        entries.insert(entry, at: entry.isPinned ? 0 : entries.firstIndex(where: { !$0.isPinned }) ?? entries.count)
        selectedIndex = min(selectedIndex, max(0, filteredEntries.count - 1))
    }

    func thumbnail(for entry: ClipboardHistoryEntry) -> NSImage? {
        imageThumbnails[entry.id]
    }

    func loadThumbnailIfNeeded(for entry: ClipboardHistoryEntry) {
        guard entry.kind == .image,
              imageThumbnails[entry.id] == nil,
              thumbnailTasks[entry.id] == nil else {
            return
        }
        let loadID = UUID()
        let task = Task { [weak self, store] in
            defer {
                if self?.thumbnailTasks[entry.id]?.id == loadID {
                    self?.thumbnailTasks[entry.id] = nil
                }
            }
            guard let payload = try? await store.payload(for: entry),
                  case let .image(data, _) = payload else {
                return
            }
            let decoded = try? await AppCancellableDetachedWork.run(priority: .utility) {
                AppImageThumbnailLoader.decode(
                    data,
                    maximumSourcePixelCount: ClipboardThumbnailPolicy.maximumSourcePixelCount,
                    maximumDisplayPixelSize: ClipboardThumbnailPolicy.maximumDisplayPixelSize
                )
            }
            guard !Task.isCancelled, let decoded else { return }
            self?.cacheThumbnail(
                NSImage(cgImage: decoded.image, size: .zero),
                for: entry.id
            )
        }
        thumbnailTasks[entry.id] = ThumbnailLoad(id: loadID, task: task)
    }

    func updateQuery(_ value: String) {
        query = value
        selectedIndex = 0
    }

    func moveSelection(_ direction: MoveCommandDirection) {
        let values = filteredEntries
        guard !values.isEmpty else { return }
        switch direction {
        case .up:
            selectedIndex = selectedIndex == 0 ? values.count - 1 : selectedIndex - 1
        case .down:
            selectedIndex = (selectedIndex + 1) % values.count
        default:
            return
        }
    }

    func select(_ index: Int) {
        guard filteredEntries.indices.contains(index) else { return }
        selectedIndex = index
    }

    func restoreSelected() {
        let values = filteredEntries
        guard values.indices.contains(selectedIndex) else { return }
        restore(values[selectedIndex])
    }

    func restore(_ entry: ClipboardHistoryEntry) {
        Task { [weak self, store] in
            do {
                let payload = try await store.payload(for: entry)
                try Self.writeToPasteboard(payload, entryID: entry.id)
                self?.errorMessage = nil
                self?.onRestoreSucceeded?()
            } catch {
                self?.errorMessage = error.localizedDescription
            }
        }
    }

    func togglePinSelected() {
        let values = filteredEntries
        guard values.indices.contains(selectedIndex) else { return }
        let entry = values[selectedIndex]
        Task { [weak self, store] in
            do {
                try await store.setPinned(!entry.isPinned, id: entry.id)
                self?.refresh()
            } catch {
                self?.errorMessage = error.localizedDescription
            }
        }
    }

    func deleteSelected() {
        let values = filteredEntries
        guard values.indices.contains(selectedIndex) else { return }
        let entry = values[selectedIndex]
        Task { [weak self, store] in
            do {
                try await store.delete(id: entry.id)
                self?.entries.removeAll { $0.id == entry.id }
                self?.imageThumbnails[entry.id] = nil
                self?.thumbnailRecency.removeAll { $0 == entry.id }
                self?.thumbnailTasks.removeValue(forKey: entry.id)?.task.cancel()
                self?.selectedIndex = min(self?.selectedIndex ?? 0, max(0, (self?.filteredEntries.count ?? 1) - 1))
            } catch {
                self?.errorMessage = error.localizedDescription
            }
        }
    }

    func clearHistory() {
        Task { [weak self, store] in
            do {
                try await store.clear()
                self?.entries = []
                self?.thumbnailTasks.values.forEach { $0.task.cancel() }
                self?.thumbnailTasks.removeAll()
                self?.imageThumbnails.removeAll()
                self?.thumbnailRecency.removeAll()
                self?.selectedIndex = 0
            } catch {
                self?.errorMessage = error.localizedDescription
            }
        }
    }

    func cancel() {
        onCancel?()
    }

    func showAutomaticPastePermissionNotice() {
        noticeMessage = "已复制到剪贴板；授权 ChatOS 使用辅助功能后，再选择一次即可自动粘贴。"
    }

    func sourceName(for entry: ClipboardHistoryEntry) -> String? {
        guard let bundleID = entry.sourceApplicationBundleID,
              let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID) else {
            return entry.sourceApplicationBundleID
        }
        return url.deletingPathExtension().lastPathComponent
    }

    private static func writeToPasteboard(
        _ payload: ClipboardHistoryPayload,
        entryID: UUID
    ) throws {
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        let wrotePayload = switch payload {
        case let .text(value):
            pasteboard.setString(value, forType: .string)
        case let .url(value):
            pasteboard.setString(value.absoluteString, forType: .URL)
                && pasteboard.setString(value.absoluteString, forType: .string)
        case let .files(values):
            pasteboard.writeObjects(values as [NSURL])
        case let .image(data, pasteboardType):
            pasteboard.setData(data, forType: NSPasteboard.PasteboardType(pasteboardType))
        }
        let wroteMarker = pasteboard.setString(
            entryID.uuidString,
            forType: ClipboardHistoryMonitor.restoredMarkerType
        )
        guard wrotePayload, wroteMarker else {
            throw ClipboardHistoryPasteError.writeFailed
        }
    }

    private func pruneThumbnailCache(validEntries: [ClipboardHistoryEntry]) {
        let validIDs = Set(validEntries.lazy.filter { $0.kind == .image }.map(\.id))
        imageThumbnails = imageThumbnails.filter { validIDs.contains($0.key) }
        thumbnailRecency.removeAll { !validIDs.contains($0) }
        let invalidTaskIDs = thumbnailTasks.keys.filter { !validIDs.contains($0) }
        for id in invalidTaskIDs {
            thumbnailTasks.removeValue(forKey: id)?.task.cancel()
        }
    }

    private func cacheThumbnail(_ image: NSImage, for entryID: UUID) {
        imageThumbnails[entryID] = image
        thumbnailRecency.removeAll { $0 == entryID }
        thumbnailRecency.append(entryID)
        while thumbnailRecency.count > ClipboardThumbnailPolicy.maximumCachedCount {
            let evictedID = thumbnailRecency.removeFirst()
            imageThumbnails[evictedID] = nil
            thumbnailTasks.removeValue(forKey: evictedID)?.task.cancel()
        }
    }
}
