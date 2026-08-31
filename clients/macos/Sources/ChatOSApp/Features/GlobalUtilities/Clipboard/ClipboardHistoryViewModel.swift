import AppKit
import ChatOSCore
import Foundation
import ImageIO
import SwiftUI

@MainActor
final class ClipboardHistoryViewModel: ObservableObject {
    @Published var query = ""
    @Published private(set) var entries: [ClipboardHistoryEntry] = []
    @Published private(set) var selectedIndex = 0
    @Published private(set) var isLoading = false
    @Published private(set) var errorMessage: String?
    @Published private(set) var imageThumbnails: [UUID: NSImage] = [:]

    var onRestoreSucceeded: (() -> Void)?
    var onCancel: (() -> Void)?

    private let store: ClipboardHistoryStore
    private var thumbnailTasks: [UUID: Task<Void, Never>] = [:]

    init(store: ClipboardHistoryStore) {
        self.store = store
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
        refresh()
    }

    func refresh() {
        isLoading = true
        Task { [weak self, store] in
            do {
                let values = try await store.entries()
                self?.entries = values
                self?.pruneThumbnailCache(validEntries: values)
                self?.selectedIndex = min(self?.selectedIndex ?? 0, max(0, values.count - 1))
                self?.errorMessage = nil
            } catch {
                self?.errorMessage = error.localizedDescription
            }
            self?.isLoading = false
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
        thumbnailTasks[entry.id] = Task { [weak self, store] in
            defer { self?.thumbnailTasks[entry.id] = nil }
            guard let payload = try? await store.payload(for: entry),
                  case let .image(data, _) = payload else {
                return
            }
            let image = await Task.detached(priority: .utility) {
                Self.makeThumbnail(data: data, maximumPixelSize: 180)
            }.value
            guard !Task.isCancelled, let image else { return }
            self?.imageThumbnails[entry.id] = image
        }
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
                self?.thumbnailTasks.removeValue(forKey: entry.id)?.cancel()
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
                self?.thumbnailTasks.values.forEach { $0.cancel() }
                self?.thumbnailTasks.removeAll()
                self?.imageThumbnails.removeAll()
                self?.selectedIndex = 0
            } catch {
                self?.errorMessage = error.localizedDescription
            }
        }
    }

    func cancel() {
        onCancel?()
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
        switch payload {
        case let .text(value):
            pasteboard.setString(value, forType: .string)
        case let .url(value):
            pasteboard.setString(value.absoluteString, forType: .URL)
            pasteboard.setString(value.absoluteString, forType: .string)
        case let .files(values):
            pasteboard.writeObjects(values as [NSURL])
        case let .image(data, pasteboardType):
            pasteboard.setData(data, forType: NSPasteboard.PasteboardType(pasteboardType))
        }
        pasteboard.setString(
            entryID.uuidString,
            forType: ClipboardHistoryMonitor.restoredMarkerType
        )
    }

    private func pruneThumbnailCache(validEntries: [ClipboardHistoryEntry]) {
        let validIDs = Set(validEntries.lazy.filter { $0.kind == .image }.map(\.id))
        imageThumbnails = imageThumbnails.filter { validIDs.contains($0.key) }
        let invalidTaskIDs = thumbnailTasks.keys.filter { !validIDs.contains($0) }
        for id in invalidTaskIDs {
            thumbnailTasks.removeValue(forKey: id)?.cancel()
        }
    }

    nonisolated private static func makeThumbnail(
        data: Data,
        maximumPixelSize: Int
    ) -> NSImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateThumbnailAtIndex(
                source,
                0,
                [
                    kCGImageSourceCreateThumbnailFromImageAlways: true,
                    kCGImageSourceCreateThumbnailWithTransform: true,
                    kCGImageSourceThumbnailMaxPixelSize: maximumPixelSize,
                    kCGImageSourceShouldCacheImmediately: true,
                ] as CFDictionary
              ) else {
            return nil
        }
        return NSImage(cgImage: image, size: NSSize(width: image.width, height: image.height))
    }
}
