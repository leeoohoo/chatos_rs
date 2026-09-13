import AppKit
import ChatOSConnector
import ChatOSCore
import Combine
import Foundation
import SwiftUI

struct QuickSearchApplicationRecord: Sendable, Hashable {
    let name: String
    let bundleIdentifier: String?
    let url: URL
}

private struct QuickSearchUsageRecord: Codable, Equatable, Sendable {
    var lastUsedAt: TimeInterval
    var count: Int
}

@MainActor
final class QuickSearchUsageStore: ObservableObject {
    @Published private(set) var persistenceError: String?

    private let persistence: NativeLocalClientSettingStore<[String: QuickSearchUsageRecord]>
    private var activeOwnerUserID: String?
    private var records: [String: QuickSearchUsageRecord] = [:]
    private var persistedRecords: [String: QuickSearchUsageRecord] = [:]
    private var mutation: UInt64 = 0
    private var persistedMutation: UInt64 = 0
    private var isStorageReady = false
    private var saveTask: Task<Void, Never>?

    init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        do {
            persistence = try NativeLocalClientSettingStore(
                key: "quick_search.usage",
                accountSession: accountSession
            )
        } catch {
            preconditionFailure("Quick Search usage storage key is invalid")
        }
    }

    func activate(ownerUserID: String) async {
        saveTask?.cancel()
        activeOwnerUserID = ownerUserID
        isStorageReady = false
        persistenceError = nil
        await persistence.reset()
        do {
            let loaded = try await persistence.load(ownerUserID: ownerUserID, defaultValue: [:])
            guard activeOwnerUserID == ownerUserID else { return }
            records = loaded
            persistedRecords = loaded
            mutation = 0
            persistedMutation = 0
            isStorageReady = true
        } catch {
            guard activeOwnerUserID == ownerUserID else { return }
            records = [:]
            persistedRecords = [:]
            persistenceError = error.localizedDescription
        }
    }

    func deactivate() async {
        saveTask?.cancel()
        saveTask = nil
        activeOwnerUserID = nil
        records = [:]
        persistedRecords = [:]
        mutation = 0
        persistedMutation = 0
        isStorageReady = false
        persistenceError = nil
        await persistence.reset()
    }

    func flush() async {
        saveTask?.cancel()
        guard isStorageReady,
              mutation > persistedMutation,
              let ownerUserID = activeOwnerUserID else { return }
        await persist(records, ownerUserID: ownerUserID, mutation: mutation)
    }

    func usageBoost(
        for id: String,
        now: TimeInterval = Date().timeIntervalSince1970
    ) -> (recency: Double, frequency: Double) {
        guard let usage = records[id] else { return (0, 0) }
        let age = max(0, now - usage.lastUsedAt)
        return (
            max(0, 70 - age / 86_400 * 8),
            min(45, log2(Double(usage.count) + 1) * 12)
        )
    }

    func recordUsage(_ id: String, now: TimeInterval = Date().timeIntervalSince1970) {
        guard isStorageReady, let ownerUserID = activeOwnerUserID else { return }
        var record = records[id] ?? QuickSearchUsageRecord(lastUsedAt: 0, count: 0)
        record.lastUsedAt = now
        record.count = min(Int.max - 1, record.count) + 1
        records[id] = record
        if records.count > 512 {
            let retained = records.sorted { $0.value.lastUsedAt > $1.value.lastUsedAt }.prefix(512)
            records = Dictionary(uniqueKeysWithValues: retained.map { ($0.key, $0.value) })
        }
        mutation &+= 1
        let expectedMutation = mutation
        let snapshot = records
        saveTask?.cancel()
        saveTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(80))
            guard !Task.isCancelled, let self else { return }
            await persist(snapshot, ownerUserID: ownerUserID, mutation: expectedMutation)
        }
    }

    private func persist(
        _ snapshot: [String: QuickSearchUsageRecord],
        ownerUserID: String,
        mutation: UInt64
    ) async {
        do {
            let committed = try await persistence.saveLatest(
                ownerUserID: ownerUserID,
                value: snapshot,
                mutation: mutation
            )
            guard activeOwnerUserID == ownerUserID else { return }
            if committed, mutation >= persistedMutation {
                persistedMutation = mutation
                persistedRecords = snapshot
            }
            if mutation == self.mutation {
                persistenceError = nil
            }
        } catch {
            guard activeOwnerUserID == ownerUserID, mutation == self.mutation else { return }
            records = persistedRecords
            isStorageReady = false
            persistenceError = error.localizedDescription
        }
    }
}

@MainActor
final class QuickSearchViewModel: ObservableObject {
    @Published var query = ""
    @Published private(set) var results: [QuickSearchResult] = []
    @Published private(set) var isSearchingFiles = false
    @Published private(set) var selectedIndex = 0
    @Published private(set) var diagnostic: String?

    var onExecute: ((QuickSearchAction) -> Void)?
    var onCancel: (() -> Void)?

    private weak var model: AppModel?
    private let fileProvider = MetadataFileSearchProvider()
    private var applications: [QuickSearchApplicationRecord] = []
    private var searchTask: Task<Void, Never>?
    private var applicationLoadTask: Task<Void, Never>?
    private var generation = UUID()
    private let usageStore: QuickSearchUsageStore
    private var cancellables = Set<AnyCancellable>()

    init(model: AppModel, usageStore: QuickSearchUsageStore) {
        self.model = model
        self.usageStore = usageStore
        usageStore.$persistenceError
            .sink { [weak self] error in
                guard let error else { return }
                self?.diagnostic = error
            }
            .store(in: &cancellables)
        applicationLoadTask = Task { [weak self] in
            let records = await Task.detached(priority: .utility) {
                Self.scanApplications()
            }.value
            guard let self else { return }
            applications = records
            rebuildMemoryResults()
        }
    }

    deinit {
        searchTask?.cancel()
        applicationLoadTask?.cancel()
    }

    func prepareForPresentation() {
        query = ""
        diagnostic = nil
        selectedIndex = 0
        rebuildMemoryResults()
    }

    func updateQuery(_ value: String) {
        query = value
        selectedIndex = 0
        diagnostic = nil
        generation = UUID()
        searchTask?.cancel()
        fileProvider.cancel()
        rebuildMemoryResults()

        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count >= 2, !trimmed.hasPrefix(">"), !trimmed.hasPrefix("@") else {
            isSearchingFiles = false
            return
        }
        let requestGeneration = generation
        searchTask = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(150))
            guard !Task.isCancelled, let self else { return }
            isSearchingFiles = true
            let files = await fileProvider.search(Self.strippedPrefix(trimmed))
            guard !Task.isCancelled, generation == requestGeneration else { return }
            isSearchingFiles = false
            mergeFileResults(files)
        }
    }

    func moveSelection(_ direction: MoveCommandDirection) {
        guard !results.isEmpty else { return }
        switch direction {
        case .up:
            selectedIndex = selectedIndex == 0 ? results.count - 1 : selectedIndex - 1
        case .down:
            selectedIndex = (selectedIndex + 1) % results.count
        default:
            return
        }
    }

    func select(_ index: Int) {
        guard results.indices.contains(index) else { return }
        selectedIndex = index
    }

    func executeSelected() {
        guard results.indices.contains(selectedIndex) else { return }
        let result = results[selectedIndex]
        recordUsage(result.id)
        onExecute?(result.action)
    }

    func execute(_ result: QuickSearchResult) {
        guard let index = results.firstIndex(where: { $0.id == result.id }) else { return }
        selectedIndex = index
        executeSelected()
    }

    func cancel() {
        onCancel?()
    }

    private func rebuildMemoryResults() {
        guard let model else {
            results = []
            return
        }
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        let effectiveQuery = Self.strippedPrefix(trimmed)
        let scope = Self.scope(for: trimmed)
        var next: [QuickSearchResult] = []

        if scope == .all || scope == .chatOS {
            next.append(contentsOf: model.projects.compactMap { project in
                makeResult(
                    id: "project:\(project.id)",
                    kind: .chatOS,
                    title: project.title,
                    subtitle: project.subtitle ?? localized("ChatOS 项目", "ChatOS Project"),
                    systemImage: "folder.fill",
                    query: effectiveQuery,
                    providerWeight: 95,
                    action: .openProject(project.id)
                )
            })
            next.append(contentsOf: model.contacts.compactMap { contact in
                makeResult(
                    id: "contact:\(contact.id)",
                    kind: .chatOS,
                    title: contact.title,
                    subtitle: contact.subtitle ?? localized("ChatOS 联系人", "ChatOS Contact"),
                    systemImage: "person.crop.circle.fill",
                    query: effectiveQuery,
                    providerWeight: 88,
                    action: .openContact(contact.id)
                )
            })
        }

        if scope == .all || scope == .applications {
            next.append(contentsOf: applications.compactMap { application in
                makeResult(
                    id: "app:\(application.bundleIdentifier ?? application.url.path)",
                    kind: .application,
                    title: application.name,
                    subtitle: application.bundleIdentifier,
                    systemImage: "app.fill",
                    query: effectiveQuery,
                    providerWeight: 74,
                    action: .openApplication(application.url)
                )
            })
        }

        if scope == .all || scope == .actions {
            next.append(contentsOf: builtInActions.compactMap { action in
                makeResult(
                    id: "action:\(action.action.rawValue)",
                    kind: .action,
                    title: action.title,
                    subtitle: action.subtitle,
                    systemImage: action.systemImage,
                    query: effectiveQuery,
                    providerWeight: 62,
                    action: .builtIn(action.action)
                )
            })
        }

        results = Array(QuickSearchRanking.sorted(next).prefix(36))
        selectedIndex = min(selectedIndex, max(0, results.count - 1))
    }

    private func mergeFileResults(_ files: [MetadataFileSearchRecord]) {
        let effectiveQuery = Self.strippedPrefix(query.trimmingCharacters(in: .whitespacesAndNewlines))
        let fileResults = files.compactMap { file in
            makeResult(
                id: "file:\(file.url.path)",
                kind: .file,
                title: file.displayName,
                subtitle: file.url.deletingLastPathComponent().path,
                systemImage: file.url.hasDirectoryPath ? "folder.fill" : "doc.fill",
                query: effectiveQuery,
                providerWeight: 38,
                action: .openFile(file.url)
            )
        }
        let withoutFiles = results.filter { $0.kind != .file }
        results = Array(QuickSearchRanking.sorted(withoutFiles + fileResults).prefix(50))
        selectedIndex = min(selectedIndex, max(0, results.count - 1))
        if files.isEmpty, withoutFiles.isEmpty {
            diagnostic = localized(
                "没有找到结果；如果 Spotlight 索引已关闭，文件结果将不可用。",
                "No results. File results are unavailable when Spotlight indexing is disabled."
            )
        }
    }

    private func makeResult(
        id: String,
        kind: QuickSearchResultKind,
        title: String,
        subtitle: String?,
        systemImage: String,
        query: String,
        providerWeight: Double,
        action: QuickSearchAction
    ) -> QuickSearchResult? {
        let usageBoost = usageBoost(for: id)
        guard let score = QuickSearchRanking.score(
            query: query,
            title: title,
            subtitle: subtitle,
            providerWeight: providerWeight,
            recencyBoost: usageBoost.recency,
            frequencyBoost: usageBoost.frequency
        ) else { return nil }
        return QuickSearchResult(
            id: id,
            kind: kind,
            title: title,
            subtitle: subtitle,
            systemImage: systemImage,
            score: score,
            action: action
        )
    }

    private var builtInActions: [(action: QuickSearchBuiltInAction, title: String, subtitle: String, systemImage: String)] {
        [
            (.screenshot, localized("截屏", "Take Screenshot"), localized("选择区域、标注或长截图", "Capture, annotate, or create a long screenshot"), "viewfinder"),
            (.screenRecording, localized("开始或停止录屏", "Start or Stop Recording"), localized("录制显示器或窗口", "Record a display or window"), "record.circle"),
            (.clipboardHistory, localized("打开剪贴板历史", "Open Clipboard History"), localized("查找并恢复之前复制的内容", "Find and restore copied content"), "clipboard"),
            (.openSettings, localized("打开 ChatOS 设置", "Open ChatOS Settings"), localized("管理账号、全局工具和连接器", "Manage account, global tools, and connector"), "gearshape.fill"),
            (.openRuntimePermissions, localized("打开系统权限", "Open Runtime & Permissions"), localized("检查屏幕录制、辅助功能和磁盘权限", "Inspect screen recording, accessibility, and disk permissions"), "lock.open.display"),
        ]
    }

    private func usageBoost(for id: String) -> (recency: Double, frequency: Double) {
        usageStore.usageBoost(for: id)
    }

    private func recordUsage(_ id: String) {
        usageStore.recordUsage(id)
    }

    private func localized(_ chinese: String, _ english: String) -> String {
        model?.interfaceLanguage == .english ? english : chinese
    }

    nonisolated private static func scanApplications() -> [QuickSearchApplicationRecord] {
        let roots = [
            URL(fileURLWithPath: "/Applications", isDirectory: true),
            URL(fileURLWithPath: "/System/Applications", isDirectory: true),
            FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Applications", isDirectory: true),
        ]
        let keys: [URLResourceKey] = [.isDirectoryKey, .isApplicationKey, .nameKey]
        var seen = Set<String>()
        var records: [QuickSearchApplicationRecord] = []
        for root in roots where FileManager.default.fileExists(atPath: root.path) {
            guard let enumerator = FileManager.default.enumerator(
                at: root,
                includingPropertiesForKeys: keys,
                options: [.skipsHiddenFiles, .skipsPackageDescendants]
            ) else { continue }
            for case let url as URL in enumerator {
                guard url.pathExtension.lowercased() == "app" else { continue }
                enumerator.skipDescendants()
                let bundle = Bundle(url: url)
                let identifier = bundle?.bundleIdentifier
                let identity = identifier ?? url.standardizedFileURL.path
                guard seen.insert(identity).inserted else { continue }
                let name = (bundle?.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String)
                    ?? (bundle?.object(forInfoDictionaryKey: "CFBundleName") as? String)
                    ?? url.deletingPathExtension().lastPathComponent
                records.append(QuickSearchApplicationRecord(
                    name: name,
                    bundleIdentifier: identifier,
                    url: url
                ))
            }
        }
        return records.sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }

    private static func strippedPrefix(_ query: String) -> String {
        guard let first = query.first, [">", "@", "/"].contains(String(first)) else { return query }
        return String(query.dropFirst()).trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func scope(for query: String) -> SearchScope {
        if query.hasPrefix(">") { return .actions }
        if query.hasPrefix("@") { return .chatOS }
        if query.hasPrefix("/") { return .files }
        return .all
    }

    private enum SearchScope {
        case all, actions, chatOS, applications, files
    }

}
