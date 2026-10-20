import ChatOSCore
import Foundation

@MainActor
final class RemoteSFTPViewModel: ObservableObject {
    nonisolated private static let maximumLocalDirectoryEntries = 10_000

    @Published private(set) var remotePath = "."
    @Published private(set) var remoteParentPath: String?
    @Published private(set) var remoteEntries: [RemoteFileEntry] = []
    @Published var selectedRemotePath: String?
    @Published private(set) var localPath: URL
    @Published private(set) var localEntries: [LocalFileEntry] = []
    @Published private(set) var isLocalListingTruncated = false
    @Published var selectedLocalPath: String?
    @Published var searchText = ""
    @Published var showsHiddenFiles = false
    @Published private(set) var isLoadingRemote = false
    @Published private(set) var isLoadingLocal = false
    @Published private(set) var isTransferring = false
    @Published private(set) var transferLabel: String?
    @Published var notice: String?
    @Published var errorMessage: String?
    @Published private(set) var refreshWarning: String?
    @Published var verificationPrompt: String?
    @Published var verificationCode = ""
    var interfaceLanguage: ChatOSLanguage = .simplifiedChinese

    let connectionID: String
    private let useCase: RemoteFileUseCase
    private var hasLoaded = false
    private var lifecycle = RemoteFileLifecycleState()
    private var remoteLoadTask: Task<Void, Never>?
    private var localLoadTask: Task<Void, Never>?
    private var mutationTask: Task<Void, Never>?

    init(connectionID: String, service: any RemoteFileServicing) {
        self.connectionID = connectionID
        self.useCase = RemoteFileUseCase(service: service)
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

    func loadIfNeeded() {
        guard !hasLoaded else { return }
        load()
    }

    func load() {
        notice = nil
        errorMessage = nil
        refreshWarning = nil
        reloadLocal()
        authenticateAndLoad(verificationCode: nil, isRetry: false)
    }

    func submitVerification() {
        let code = verificationCode.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty else {
            errorMessage = localized(
                "请输入验证码。",
                english: "Enter the verification code."
            )
            return
        }
        authenticateAndLoad(verificationCode: code, isRetry: true)
    }

    func cancelVerification() {
        remoteLoadTask?.cancel()
        remoteLoadTask = nil
        lifecycle.cancelRemoteLoad()
        isLoadingRemote = false
        hasLoaded = false
        verificationPrompt = nil
        verificationCode = ""
        errorMessage = localized(
            "SSH 二次验证已取消。",
            english: "SSH verification was cancelled."
        )
    }

    private func authenticateAndLoad(
        verificationCode: String?,
        isRetry: Bool
    ) {
        guard let token = beginRemoteLoad(target: .initial) else { return }
        let useCase = useCase
        let connectionID = connectionID
        remoteLoadTask = Task { [weak self] in
            let result: Result<RemoteDirectoryListing, Error>
            do {
                result = .success(try await useCase.connect(
                    connectionID: connectionID,
                    verificationCode: verificationCode
                ))
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.lifecycle.acceptsRemoteLoad(token) else { return }
            switch result {
            case let .success(listing):
                self.applyRemoteListing(listing)
                self.hasLoaded = true
                self.verificationPrompt = nil
                self.verificationCode = ""
            case let .failure(challenge as RemoteVerificationChallenge):
                self.hasLoaded = false
                self.verificationPrompt = challenge.prompt
                self.verificationCode = ""
                if isRetry {
                    self.errorMessage = self.localized(
                        "验证码未通过，请重新输入。",
                        english: "The verification code was rejected. Try again."
                    )
                }
            case let .failure(error):
                self.hasLoaded = false
                self.errorMessage = error.localizedDescription
            }
            self.finishRemoteLoad(token)
        }
    }

    func loadRemote(path: String) {
        guard let token = beginRemoteLoad(target: .remote(path: path)) else { return }
        let useCase = useCase
        let connectionID = connectionID
        remoteLoadTask = Task { [weak self] in
            let result: Result<RemoteDirectoryListing, Error>
            do {
                result = .success(try await useCase.list(
                    connectionID: connectionID,
                    path: path
                ))
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.lifecycle.acceptsRemoteLoad(token) else { return }
            switch result {
            case let .success(listing):
                self.applyRemoteListing(listing)
            case let .failure(error):
                self.errorMessage = error.localizedDescription
            }
            self.finishRemoteLoad(token)
        }
    }

    func reloadRemote() {
        loadRemote(path: remotePath)
    }

    func openRemote(_ entry: RemoteFileEntry) {
        if entry.isDirectory {
            loadRemote(path: entry.path)
        } else {
            selectedRemotePath = entry.path
        }
    }

    func goToRemoteParent() {
        guard let remoteParentPath else { return }
        loadRemote(path: remoteParentPath)
    }

    func setLocalDirectory(_ url: URL) {
        localPath = url.standardizedFileURL
        reloadLocal()
    }

    func reloadLocal() {
        let directory = localPath
        guard let token = lifecycle.beginLocalLoad(
            connectionID: connectionID,
            path: directory.path
        ) else { return }
        localLoadTask?.cancel()
        isLoadingLocal = true
        errorMessage = nil
        localLoadTask = Task { [weak self] in
            let result: Result<LocalDirectoryReadResult, Error>
            do {
                result = .success(try await AppCancellableDetachedWork.run {
                    try Self.readLocalDirectory(directory)
                })
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.lifecycle.acceptsLocalLoad(token) else { return }
            switch result {
            case let .success(listing):
                self.applyLocalListing(listing)
            case let .failure(error):
                self.errorMessage = self.localized(
                    "无法读取本机目录：\(error.localizedDescription)",
                    english: "Unable to read the local folder: \(error.localizedDescription)"
                )
            }
            self.finishLocalLoad(token)
        }
    }

    func openLocal(_ entry: LocalFileEntry) {
        if entry.isDirectory {
            localPath = entry.url
            reloadLocal()
        } else {
            selectedLocalPath = entry.id
        }
    }

    func goToLocalParent() {
        let parent = localPath.deletingLastPathComponent()
        guard parent.path != localPath.path else { return }
        localPath = parent
        reloadLocal()
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

    func uploadSelected(overwrite: Bool) {
        guard let entry = selectedLocalEntry, !entry.isDirectory else { return }
        beginMutation(
            plan: .upload(
                localURL: entry.url,
                remoteDirectory: remotePath,
                overwrite: overwrite
            ),
            kind: .upload(remoteDirectory: remotePath),
            label: localized(
                "正在上传 \(entry.name)…",
                english: "Uploading \(entry.name)…"
            ),
            successNotice: localized(
                "已上传 \(entry.name)",
                english: "Uploaded \(entry.name)"
            )
        )
    }

    func downloadSelected(overwrite: Bool) {
        guard let entry = selectedRemoteEntry, entry.kind == .file else { return }
        let target = localPath.appendingPathComponent(entry.name)
        beginMutation(
            plan: .download(
                remotePath: entry.path,
                localURL: target,
                localDirectory: localPath,
                overwrite: overwrite
            ),
            kind: .download(remotePath: entry.path),
            label: localized(
                "正在下载 \(entry.name)…",
                english: "Downloading \(entry.name)…"
            ),
            successNotice: localized(
                "已下载到 \(target.path)",
                english: "Downloaded to \(target.path)"
            )
        )
    }

    func createRemoteDirectory(name: String) {
        beginMutation(
            plan: .createDirectory(parentPath: remotePath, name: name),
            kind: .createDirectory(parentPath: remotePath),
            label: localized("正在创建目录…", english: "Creating folder…"),
            successNotice: localized(
                "已创建目录 \(name)",
                english: "Created folder \(name)"
            )
        )
    }

    func renameSelectedRemote(to newName: String) {
        guard let entry = selectedRemoteEntry else { return }
        beginMutation(
            plan: .rename(path: entry.path, newName: newName, parentPath: remotePath),
            kind: .rename(path: entry.path),
            label: localized("正在重命名…", english: "Renaming…"),
            successNotice: localized(
                "已重命名为 \(newName)",
                english: "Renamed to \(newName)"
            )
        )
    }

    func deleteSelectedRemote() {
        guard let entry = selectedRemoteEntry else { return }
        beginMutation(
            plan: .delete(
                path: entry.path,
                recursively: entry.isDirectory,
                parentPath: remotePath
            ),
            kind: .delete(path: entry.path),
            label: localized("正在删除…", english: "Deleting…"),
            successNotice: localized(
                "已删除 \(entry.name)",
                english: "Deleted \(entry.name)"
            )
        )
    }

    func dismissFeedback() {
        notice = nil
        errorMessage = nil
        refreshWarning = nil
    }

    func deactivate() {
        remoteLoadTask?.cancel()
        localLoadTask?.cancel()
        mutationTask?.cancel()
        remoteLoadTask = nil
        localLoadTask = nil
        mutationTask = nil
        lifecycle.reset()
        isLoadingRemote = false
        isLoadingLocal = false
        isTransferring = false
        transferLabel = nil
    }

    private func beginRemoteLoad(
        target: RemoteFileLoadTarget
    ) -> RemoteFileRequestToken? {
        guard let token = lifecycle.beginRemoteLoad(
            connectionID: connectionID,
            target: target
        ) else { return nil }
        remoteLoadTask?.cancel()
        isLoadingRemote = true
        errorMessage = nil
        refreshWarning = nil
        return token
    }

    private func finishRemoteLoad(_ token: RemoteFileRequestToken) {
        guard lifecycle.finishRemoteLoad(token) else { return }
        remoteLoadTask = nil
        isLoadingRemote = false
    }

    private func finishLocalLoad(_ token: RemoteFileRequestToken) {
        guard lifecycle.finishLocalLoad(token) else { return }
        localLoadTask = nil
        isLoadingLocal = false
    }

    private func beginMutation(
        plan: MutationPlan,
        kind: RemoteFileMutationKind,
        label: String,
        successNotice: String
    ) {
        guard let token = lifecycle.beginMutation(
            connectionID: connectionID,
            kind: kind
        ) else { return }
        remoteLoadTask?.cancel()
        localLoadTask?.cancel()
        remoteLoadTask = nil
        localLoadTask = nil
        isLoadingRemote = false
        isLoadingLocal = false
        isTransferring = true
        transferLabel = label
        notice = nil
        errorMessage = nil
        refreshWarning = nil
        let useCase = useCase
        let connectionID = connectionID
        mutationTask = Task { [weak self] in
            let result: Result<MutationOutcome, Error>
            do {
                result = .success(try await Self.execute(
                    plan,
                    connectionID: connectionID,
                    useCase: useCase
                ))
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.lifecycle.acceptsMutation(token) else { return }
            switch result {
            case let .success(outcome):
                if let listing = outcome.remoteListing {
                    self.applyRemoteListing(listing)
                }
                if let listing = outcome.localListing {
                    self.applyLocalListing(listing)
                }
                self.notice = successNotice
                if !outcome.refreshIsComplete {
                    self.refreshWarning = self.localized(
                        "操作已完成，但目录刷新失败，请手动刷新。",
                        english: "The operation completed, but the folder could not be refreshed. Refresh it manually."
                    )
                }
            case let .failure(error):
                self.errorMessage = error.localizedDescription
            }
            self.finishMutation(token)
        }
    }

    private func finishMutation(_ token: RemoteFileMutationToken) {
        guard lifecycle.finishMutation(token) else { return }
        mutationTask = nil
        isTransferring = false
        transferLabel = nil
    }

    private func applyRemoteListing(_ listing: RemoteDirectoryListing) {
        remotePath = listing.path
        remoteParentPath = listing.parentPath
        remoteEntries = listing.entries
        selectedRemotePath = nil
    }

    private func applyLocalListing(_ listing: LocalDirectoryReadResult) {
        localEntries = listing.entries
        isLocalListingTruncated = listing.truncated
        selectedLocalPath = nil
    }

    nonisolated private static func execute(
        _ plan: MutationPlan,
        connectionID: String,
        useCase: RemoteFileUseCase
    ) async throws -> MutationOutcome {
        switch plan {
        case let .upload(localURL, remoteDirectory, overwrite):
            let result = try await useCase.upload(
                connectionID: connectionID,
                localURL: localURL,
                remoteDirectory: remoteDirectory,
                overwrite: overwrite
            )
            return .init(
                remoteListing: result.refreshedRemoteListing,
                localListing: nil,
                refreshIsComplete: result.refreshedRemoteListing != nil
            )
        case let .download(remotePath, localURL, localDirectory, overwrite):
            try await useCase.download(
                connectionID: connectionID,
                remotePath: remotePath,
                localURL: localURL,
                overwrite: overwrite
            )
            let listing = try? await AppCancellableDetachedWork.run {
                try readLocalDirectory(localDirectory)
            }
            return .init(
                remoteListing: nil,
                localListing: listing,
                refreshIsComplete: listing != nil
            )
        case let .createDirectory(parentPath, name):
            let result = try await useCase.createDirectory(
                connectionID: connectionID,
                parentPath: parentPath,
                name: name
            )
            return remoteMutationOutcome(result)
        case let .rename(path, newName, parentPath):
            let result = try await useCase.rename(
                connectionID: connectionID,
                path: path,
                newName: newName,
                parentPath: parentPath
            )
            return remoteMutationOutcome(result)
        case let .delete(path, recursively, parentPath):
            let result = try await useCase.delete(
                connectionID: connectionID,
                path: path,
                recursively: recursively,
                parentPath: parentPath
            )
            return remoteMutationOutcome(result)
        }
    }

    nonisolated private static func remoteMutationOutcome(
        _ result: RemoteFileMutationResult
    ) -> MutationOutcome {
        .init(
            remoteListing: result.refreshedRemoteListing,
            localListing: nil,
            refreshIsComplete: result.refreshedRemoteListing != nil
        )
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

    nonisolated private static func readLocalDirectory(
        _ directory: URL
    ) throws -> LocalDirectoryReadResult {
        try Task.checkCancellation()
        let keys: Set<URLResourceKey> = [
            .isDirectoryKey,
            .fileSizeKey,
            .contentModificationDateKey,
        ]
        guard let enumerator = FileManager.default.enumerator(
            at: directory,
            includingPropertiesForKeys: Array(keys),
            options: [.skipsSubdirectoryDescendants]
        ) else {
            throw CocoaError(.fileReadNoSuchFile)
        }
        var entries: [LocalFileEntry] = []
        entries.reserveCapacity(1_024)
        var truncated = false
        for case let url as URL in enumerator {
            try Task.checkCancellation()
            guard entries.count < maximumLocalDirectoryEntries else {
                truncated = true
                break
            }
            let values = try url.resourceValues(forKeys: keys)
            entries.append(LocalFileEntry(
                url: url,
                name: url.lastPathComponent,
                isDirectory: values.isDirectory == true,
                size: values.isDirectory == true ? nil : values.fileSize.map(Int64.init),
                modifiedAt: values.contentModificationDate
            ))
        }
        try Task.checkCancellation()
        entries.sort { lhs, rhs in
            if lhs.isDirectory != rhs.isDirectory { return lhs.isDirectory }
            return lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
        }
        try Task.checkCancellation()
        return .init(entries: entries, truncated: truncated)
    }
}

private enum MutationPlan: Sendable {
    case upload(localURL: URL, remoteDirectory: String, overwrite: Bool)
    case download(
        remotePath: String,
        localURL: URL,
        localDirectory: URL,
        overwrite: Bool
    )
    case createDirectory(parentPath: String, name: String)
    case rename(path: String, newName: String, parentPath: String)
    case delete(path: String, recursively: Bool, parentPath: String)
}

private struct MutationOutcome: Sendable {
    let remoteListing: RemoteDirectoryListing?
    let localListing: LocalDirectoryReadResult?
    let refreshIsComplete: Bool
}

private struct LocalDirectoryReadResult: Sendable {
    var entries: [LocalFileEntry]
    var truncated: Bool
}
