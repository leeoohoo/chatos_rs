import ChatOSConnector
import ChatOSCore
import CryptoKit
import Foundation

enum ClipboardHistoryStoreError: LocalizedError {
    case storageUnavailable
    case storage(String)
    case payloadMissing
    case invalidRecord

    var errorDescription: String? {
        switch self {
        case .storageUnavailable: "Clipboard history storage is unavailable."
        case let .storage(message): message
        case .payloadMissing: "The clipboard payload is no longer available."
        case .invalidRecord: "Clipboard history returned an invalid record."
        }
    }
}

protocol ClipboardHistoryIPCClient: Sendable {
    func clipboardEntry(id: String) async throws -> LocalAgentClipboardSnapshot
    func clipboardEntries() async throws -> [LocalAgentClipboardSnapshot]
    func storeClipboardEntry(
        id: String,
        draft: LocalAgentClipboardDraft
    ) async throws -> LocalAgentClipboardMutationResult
    func setClipboardEntryPinned(
        id: String,
        expectedRevision: UInt64,
        isPinned: Bool
    ) async throws -> LocalAgentClipboardMutationResult
    func deleteClipboardEntry(
        id: String,
        expectedRevision: UInt64
    ) async throws -> LocalAgentClipboardMutationResult
}

extension NativeLocalAgentIPCClient: ClipboardHistoryIPCClient {}

struct ClipboardHistoryStorageContext: Sendable {
    let ownerUserID: String
    let client: any ClipboardHistoryIPCClient
}

/// Clipboard bytes remain in the account's private client filesystem. All
/// searchable metadata, ordering, pinning and retention live exclusively in
/// the selected Rust Client Storage provider through typed Host IPC.
actor ClipboardHistoryStore {
    typealias ContextProvider = @MainActor @Sendable () async throws -> ClipboardHistoryStorageContext

    private let rootURL: URL
    private let payloadDirectoryURL: URL
    private let contextProvider: ContextProvider

    init(
        rootURL: URL = ClipboardHistoryStore.defaultRootURL,
        contextProvider: @escaping ContextProvider
    ) {
        self.rootURL = rootURL.standardizedFileURL
        self.payloadDirectoryURL = rootURL
            .appendingPathComponent("Payloads", isDirectory: true)
            .standardizedFileURL
        self.contextProvider = contextProvider
    }

    func add(
        payload: ClipboardHistoryPayload,
        contentHash: String,
        preview: String?,
        sourceBundleID: String?
    ) async throws -> ClipboardHistoryEntry {
        let context = try await context()
        let id = UUID()
        let encoded = try Self.encode(payload: payload)
        let accountDirectory = Self.accountPayloadDirectory(context.ownerUserID)
        let payloadReference = "Payloads/\(accountDirectory)/\(id.uuidString.lowercased()).\(encoded.extensionName)"
        let payloadURL = try payloadURL(for: payloadReference)
        try FileManager.default.createDirectory(
            at: payloadURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try encoded.data.write(to: payloadURL, options: [.atomic])

        let result = try await context.client.storeClipboardEntry(
            id: id.uuidString.lowercased(),
            draft: LocalAgentClipboardDraft(
                kind: Self.wireKind(payload.kind),
                mimeType: encoded.mimeType,
                contentHash: contentHash,
                textPreview: preview,
                sourceBundleID: sourceBundleID,
                payloadReference: payloadReference,
                byteCount: UInt64(encoded.data.count),
                pasteboardType: encoded.pasteboardType
            )
        )
        removePayloads(
            result.discardedPayloadReferences,
            expectedOwnerUserID: context.ownerUserID
        )
        guard let snapshot = result.entry else {
            throw ClipboardHistoryStoreError.invalidRecord
        }
        return try localEntry(snapshot, expectedOwnerUserID: context.ownerUserID)
    }

    func entries(limit: Int = 500) async throws -> [ClipboardHistoryEntry] {
        let context = try await context()
        let maximum = min(500, max(1, limit))
        return try await context.client.clipboardEntries()
            .prefix(maximum)
            .map { try localEntry($0, expectedOwnerUserID: context.ownerUserID) }
    }

    func payload(for entry: ClipboardHistoryEntry) async throws -> ClipboardHistoryPayload {
        let context = try await context()
        let snapshot = try await context.client.clipboardEntry(
            id: entry.id.uuidString.lowercased()
        )
        let current = try localEntry(snapshot, expectedOwnerUserID: context.ownerUserID)
        guard current.kind == entry.kind,
              current.contentHash == entry.contentHash,
              current.payloadReference == entry.payloadReference,
              current.byteCount == entry.byteCount else {
            throw ClipboardHistoryStoreError.invalidRecord
        }
        let url = try payloadURL(for: entry.payloadReference)
        guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey]),
              values.isRegularFile == true,
              values.isSymbolicLink != true,
              let data = try? Data(contentsOf: url, options: [.mappedIfSafe]) else {
            throw ClipboardHistoryStoreError.payloadMissing
        }
        switch entry.kind {
        case .text:
            return .text(String(decoding: data, as: UTF8.self))
        case .url:
            guard let value = URL(string: String(decoding: data, as: UTF8.self)) else {
                throw ClipboardHistoryStoreError.payloadMissing
            }
            return .url(value)
        case .files:
            return .files(try JSONDecoder().decode([URL].self, from: data))
        case .image:
            guard snapshot.draft.kind == .image,
                  let pasteboardType = snapshot.draft.pasteboardType else {
                throw ClipboardHistoryStoreError.invalidRecord
            }
            return .image(data: data, pasteboardType: pasteboardType)
        }
    }

    func setPinned(_ pinned: Bool, id: UUID) async throws {
        let context = try await context()
        let current = try await context.client.clipboardEntry(id: id.uuidString.lowercased())
        let result = try await context.client.setClipboardEntryPinned(
            id: current.entryID,
            expectedRevision: current.revision,
            isPinned: pinned
        )
        removePayloads(
            result.discardedPayloadReferences,
            expectedOwnerUserID: context.ownerUserID
        )
    }

    func delete(id: UUID) async throws {
        let context = try await context()
        let current: LocalAgentClipboardSnapshot
        do {
            current = try await context.client.clipboardEntry(id: id.uuidString.lowercased())
        } catch NativeLocalAgentIPCError.rejected(let error)
            where error.code == "clipboard_not_found"
        {
            return
        }
        let result = try await context.client.deleteClipboardEntry(
            id: current.entryID,
            expectedRevision: current.revision
        )
        removePayloads(
            result.discardedPayloadReferences,
            expectedOwnerUserID: context.ownerUserID
        )
    }

    func clear() async throws {
        let context = try await context()
        for entry in try await context.client.clipboardEntries() {
            let result = try await context.client.deleteClipboardEntry(
                id: entry.entryID,
                expectedRevision: entry.revision
            )
            removePayloads(
                result.discardedPayloadReferences,
                expectedOwnerUserID: context.ownerUserID
            )
        }
    }

    private func context() async throws -> ClipboardHistoryStorageContext {
        do {
            let context = try await contextProvider()
            guard !context.ownerUserID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
                throw ClipboardHistoryStoreError.storageUnavailable
            }
            return context
        } catch let error as ClipboardHistoryStoreError {
            throw error
        } catch {
            throw ClipboardHistoryStoreError.storage(error.localizedDescription)
        }
    }

    private func localEntry(
        _ snapshot: LocalAgentClipboardSnapshot,
        expectedOwnerUserID: String
    ) throws -> ClipboardHistoryEntry {
        guard snapshot.ownerUserID == expectedOwnerUserID,
              let id = UUID(uuidString: snapshot.entryID),
              let createdAt = Self.date(snapshot.createdAt),
              let updatedAt = Self.date(snapshot.updatedAt),
              updatedAt >= createdAt,
              let byteCount = Int64(exactly: snapshot.draft.byteCount) else {
            throw ClipboardHistoryStoreError.invalidRecord
        }
        let components = snapshot.draft.payloadReference.split(separator: "/")
        guard components.count == 3,
              components[1] == Substring(Self.accountPayloadDirectory(expectedOwnerUserID)) else {
            throw ClipboardHistoryStoreError.invalidRecord
        }
        _ = try payloadURL(for: snapshot.draft.payloadReference)
        return ClipboardHistoryEntry(
            id: id,
            kind: Self.localKind(snapshot.draft.kind),
            createdAt: createdAt,
            updatedAt: updatedAt,
            contentHash: snapshot.draft.contentHash,
            textPreview: snapshot.draft.textPreview,
            sourceApplicationBundleID: snapshot.draft.sourceBundleID,
            payloadReference: snapshot.draft.payloadReference,
            byteCount: byteCount,
            isPinned: snapshot.isPinned
        )
    }

    private func payloadURL(for reference: String) throws -> URL {
        let components = reference.split(separator: "/", omittingEmptySubsequences: false)
        guard components.count == 3,
              components[0] == "Payloads",
              components[1].count == 64,
              components[1].allSatisfy({ $0.isHexDigit }),
              !components[2].isEmpty,
              !reference.contains("\\"),
              !reference.contains(":"),
              !reference.contains("..") else {
            throw ClipboardHistoryStoreError.invalidRecord
        }
        let candidate = rootURL.appendingPathComponent(reference).standardizedFileURL
        let prefix = payloadDirectoryURL.path.hasSuffix("/")
            ? payloadDirectoryURL.path
            : payloadDirectoryURL.path + "/"
        guard candidate.path.hasPrefix(prefix) else {
            throw ClipboardHistoryStoreError.invalidRecord
        }
        return candidate
    }

    private func removePayloads(
        _ references: [String],
        expectedOwnerUserID: String
    ) {
        let expectedDirectory = Substring(Self.accountPayloadDirectory(expectedOwnerUserID))
        for reference in references {
            let components = reference.split(separator: "/")
            guard components.count == 3,
                  components[1] == expectedDirectory else { continue }
            guard let url = try? payloadURL(for: reference) else { continue }
            try? FileManager.default.removeItem(at: url)
        }
    }

    private static func encode(
        payload: ClipboardHistoryPayload
    ) throws -> (data: Data, extensionName: String, mimeType: String, pasteboardType: String?) {
        switch payload {
        case let .text(value):
            return (Data(value.utf8), "txt", "text/plain", nil)
        case let .url(value):
            return (Data(value.absoluteString.utf8), "url", "text/uri-list", nil)
        case let .files(values):
            return (
                try JSONEncoder().encode(values),
                "files",
                "application/vnd.chatos.file-list+json",
                nil
            )
        case let .image(data, pasteboardType):
            let lowercased = pasteboardType.lowercased()
            let type: (String, String)
            if lowercased.contains("jpeg") || lowercased.contains("jpg") {
                type = ("jpeg", "image/jpeg")
            } else if lowercased.contains("tiff") {
                type = ("tiff", "image/tiff")
            } else {
                type = ("png", "image/png")
            }
            return (data, type.0, type.1, pasteboardType)
        }
    }

    private static func wireKind(_ kind: ClipboardContentKind) -> LocalAgentClipboardKind {
        switch kind {
        case .text: .text
        case .url: .url
        case .files: .files
        case .image: .image
        }
    }

    private static func localKind(_ kind: LocalAgentClipboardKind) -> ClipboardContentKind {
        switch kind {
        case .text: .text
        case .url: .url
        case .files: .files
        case .image: .image
        }
    }

    private static func date(_ value: String) -> Date? {
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractional.date(from: value) ?? ISO8601DateFormatter().date(from: value)
    }

    private static func accountPayloadDirectory(_ ownerUserID: String) -> String {
        SHA256.hash(data: Data(ownerUserID.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
    }

    static let defaultRootURL: URL = FileManager.default.urls(
        for: .applicationSupportDirectory,
        in: .userDomainMask
    )[0]
        .appendingPathComponent("ChatOS", isDirectory: true)
        .appendingPathComponent("ClipboardHistoryV2", isDirectory: true)
}
