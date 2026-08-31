import AppKit
import ChatOSCore
import Foundation
import SwiftUI

struct QuickSearchApplicationRecord: Sendable, Hashable {
    let name: String
    let bundleIdentifier: String?
    let url: URL
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
    private var usage: [String: UsageRecord]
    private let usageDefaultsKey = "ChatOS.quickSearch.usage"

    init(model: AppModel) {
        self.model = model
        self.usage = Self.loadUsage(key: usageDefaultsKey)
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
        guard let usage = usage[id] else { return (0, 0) }
        let age = max(0, Date().timeIntervalSince1970 - usage.lastUsedAt)
        let recency = max(0, 70 - age / 86_400 * 8)
        let frequency = min(45, log2(Double(usage.count) + 1) * 12)
        return (recency, frequency)
    }

    private func recordUsage(_ id: String) {
        var record = usage[id] ?? UsageRecord(lastUsedAt: 0, count: 0)
        record.lastUsedAt = Date().timeIntervalSince1970
        record.count += 1
        usage[id] = record
        if let data = try? JSONEncoder().encode(usage) {
            UserDefaults.standard.set(data, forKey: usageDefaultsKey)
        }
    }

    private func localized(_ chinese: String, _ english: String) -> String {
        model?.interfaceLanguage == .english ? english : chinese
    }

    private static func loadUsage(key: String) -> [String: UsageRecord] {
        guard let data = UserDefaults.standard.data(forKey: key) else { return [:] }
        return (try? JSONDecoder().decode([String: UsageRecord].self, from: data)) ?? [:]
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

    private struct UsageRecord: Codable {
        var lastUsedAt: TimeInterval
        var count: Int
    }
}
