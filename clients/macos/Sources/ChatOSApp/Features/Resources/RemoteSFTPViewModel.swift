import ChatOSCore
import Foundation

@MainActor
final class RemoteSFTPViewModel: ObservableObject {
    @Published private(set) var remotePath = "."
    @Published private(set) var remoteParentPath: String?
    @Published private(set) var remoteEntries: [RemoteFileEntry] = []
    @Published var selectedRemotePath: String?
    @Published private(set) var localPath: URL
    @Published private(set) var localEntries: [LocalFileEntry] = []
    @Published var selectedLocalPath: String?
    @Published var searchText = ""
    @Published var showsHiddenFiles = false
    @Published private(set) var isLoadingRemote = false
    @Published private(set) var isLoadingLocal = false
    @Published private(set) var isTransferring = false
    @Published private(set) var transferLabel: String?
    @Published var notice: String?
    @Published var errorMessage: String?
    var interfaceLanguage: ChatOSLanguage = .simplifiedChinese

    let connectionID: String
    private let service: any RemoteFileServicing
    private var hasLoaded = false

    init(connectionID: String, service: any RemoteFileServicing) {
        self.connectionID = connectionID
        self.service = service
        self.localPath = Self.defaultLocalDirectory()
    }

    var visibleRemoteEntries: [RemoteFileEntry] {
        remoteEntries.filter { entry in
            isVisible(name: entry.name) && matchesSearch(entry.name)
        }
    }

    var visibleLocalEntries: [LocalFileEntry] {
        localEntries.filter { entry in
            isVisible(name: entry.name) && matchesSearch(entry.name)
        }
    }

    var selectedRemoteEntry: RemoteFileEntry? {
        guard let selectedRemotePath else { return nil }
        return remoteEntries.first { $0.path == selectedRemotePath }
    }

    var selectedLocalEntry: LocalFileEntry? {
        guard let selectedLocalPath else { return nil }
        return localEntries.first { $0.id == selectedLocalPath }
    }

    var canUpload: Bool {
        selectedLocalEntry?.isDirectory == false && !isTransferring
    }

    var canDownload: Bool {
        selectedRemoteEntry?.kind == .file && !isTransferring
    }

    func loadIfNeeded() async {
        guard !hasLoaded else { return }
        await load()
    }

    func load() async {
        notice = nil
        errorMessage = nil
        await reloadLocal()
        do {
            isLoadingRemote = true
            let initial = try await service.initialDirectory(connectionID: connectionID)
            await loadRemote(path: initial)
            hasLoaded = errorMessage == nil
        } catch {
            isLoadingRemote = false
            hasLoaded = false
            errorMessage = error.localizedDescription
        }
    }

    func loadRemote(path: String) async {
        isLoadingRemote = true
        errorMessage = nil
        do {
            let listing = try await service.listDirectory(connectionID: connectionID, path: path)
            remotePath = listing.path
            remoteParentPath = listing.parentPath
            remoteEntries = listing.entries
            selectedRemotePath = nil
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoadingRemote = false
    }

    func reloadRemote() async {
        await loadRemote(path: remotePath)
    }

    func openRemote(_ entry: RemoteFileEntry) async {
        if entry.isDirectory {
            await loadRemote(path: entry.path)
        } else {
            selectedRemotePath = entry.path
        }
    }

    func goToRemoteParent() async {
        guard let remoteParentPath else { return }
        await loadRemote(path: remoteParentPath)
    }

    func setLocalDirectory(_ url: URL) async {
        localPath = url.standardizedFileURL
        await reloadLocal()
    }

    func reloadLocal() async {
        isLoadingLocal = true
        errorMessage = nil
        let directory = localPath
        do {
            localEntries = try await Task.detached(priority: .userInitiated) {
                try Self.readLocalDirectory(directory)
            }.value
            selectedLocalPath = nil
        } catch {
            errorMessage = localized(
                "无法读取本机目录：\(error.localizedDescription)",
                english: "Unable to read the local folder: \(error.localizedDescription)"
            )
        }
        isLoadingLocal = false
    }

    func openLocal(_ entry: LocalFileEntry) async {
        if entry.isDirectory {
            localPath = entry.url
            await reloadLocal()
        } else {
            selectedLocalPath = entry.id
        }
    }

    func goToLocalParent() async {
        let parent = localPath.deletingLastPathComponent()
        guard parent.path != localPath.path else { return }
        localPath = parent
        await reloadLocal()
    }

    func overwriteActionForUpload() -> RemoteSFTPOverwriteAction? {
        guard let selectedLocalEntry else { return nil }
        return remoteEntries.contains(where: { $0.name == selectedLocalEntry.name })
            ? .upload(selectedLocalEntry)
            : nil
    }

    func overwriteActionForDownload() -> RemoteSFTPOverwriteAction? {
        guard let selectedRemoteEntry else { return nil }
        let target = localPath.appendingPathComponent(selectedRemoteEntry.name)
        return FileManager.default.fileExists(atPath: target.path)
            ? .download(selectedRemoteEntry)
            : nil
    }

    func uploadSelected(overwrite: Bool) async {
        guard let entry = selectedLocalEntry, !entry.isDirectory else { return }
        await performTransfer(label: localized(
            "正在上传 \(entry.name)…",
            english: "Uploading \(entry.name)…"
        )) {
            _ = try await service.uploadFile(
                connectionID: connectionID,
                localURL: entry.url,
                remoteDirectory: remotePath,
                overwrite: overwrite
            )
            await reloadRemote()
            notice = localized("已上传 \(entry.name)", english: "Uploaded \(entry.name)")
        }
    }

    func downloadSelected(overwrite: Bool) async {
        guard let entry = selectedRemoteEntry, entry.kind == .file else { return }
        let target = localPath.appendingPathComponent(entry.name)
        await performTransfer(label: localized(
            "正在下载 \(entry.name)…",
            english: "Downloading \(entry.name)…"
        )) {
            try await service.downloadFile(
                connectionID: connectionID,
                remotePath: entry.path,
                localURL: target,
                overwrite: overwrite
            )
            await reloadLocal()
            notice = localized("已下载到 \(target.path)", english: "Downloaded to \(target.path)")
        }
    }

    func createRemoteDirectory(name: String) async {
        await performRemoteAction {
            try await service.createDirectory(
                connectionID: connectionID,
                parentPath: remotePath,
                name: name
            )
            await reloadRemote()
            notice = localized("已创建目录 \(name)", english: "Created folder \(name)")
        }
    }

    func renameSelectedRemote(to newName: String) async {
        guard let entry = selectedRemoteEntry else { return }
        await performRemoteAction {
            try await service.renameEntry(
                connectionID: connectionID,
                path: entry.path,
                newName: newName
            )
            await reloadRemote()
            notice = localized("已重命名为 \(newName)", english: "Renamed to \(newName)")
        }
    }

    func deleteSelectedRemote() async {
        guard let entry = selectedRemoteEntry else { return }
        await performRemoteAction {
            try await service.deleteEntry(
                connectionID: connectionID,
                path: entry.path,
                recursively: entry.isDirectory
            )
            await reloadRemote()
            notice = localized("已删除 \(entry.name)", english: "Deleted \(entry.name)")
        }
    }

    private func performTransfer(
        label: String,
        operation: () async throws -> Void
    ) async {
        guard !isTransferring else { return }
        isTransferring = true
        transferLabel = label
        notice = nil
        errorMessage = nil
        do { try await operation() }
        catch { errorMessage = error.localizedDescription }
        transferLabel = nil
        isTransferring = false
    }

    private func performRemoteAction(operation: () async throws -> Void) async {
        notice = nil
        errorMessage = nil
        do { try await operation() }
        catch { errorMessage = error.localizedDescription }
    }

    private func matchesSearch(_ name: String) -> Bool {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        return query.isEmpty || name.localizedCaseInsensitiveContains(query)
    }

    private func isVisible(name: String) -> Bool {
        showsHiddenFiles || !name.hasPrefix(".")
    }

    private func localized(_ chinese: String, english: String) -> String {
        interfaceLanguage == .english ? english : chinese
    }

    private static func defaultLocalDirectory() -> URL {
        FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
            ?? FileManager.default.homeDirectoryForCurrentUser
    }

    nonisolated private static func readLocalDirectory(_ directory: URL) throws -> [LocalFileEntry] {
        let keys: Set<URLResourceKey> = [
            .isDirectoryKey,
            .fileSizeKey,
            .contentModificationDateKey,
        ]
        return try FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: Array(keys),
            options: []
        ).map { url in
            let values = try url.resourceValues(forKeys: keys)
            return LocalFileEntry(
                url: url,
                name: url.lastPathComponent,
                isDirectory: values.isDirectory == true,
                size: values.isDirectory == true ? nil : values.fileSize.map(Int64.init),
                modifiedAt: values.contentModificationDate
            )
        }.sorted { lhs, rhs in
            if lhs.isDirectory != rhs.isDirectory { return lhs.isDirectory }
            return lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
        }
    }
}
