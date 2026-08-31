import ChatOSCore
import Foundation

@MainActor
final class PetOverlayStore: ObservableObject {
    @Published private(set) var presentation: PetPresentation = .idle
    @Published private(set) var activities: [PetActivity] = []

    private var reducer = PetStateReducer()
    private var expirationTask: Task<Void, Never>?
    private var sourceVersions: [PetActivitySource: Int64] = [:]
    private var dismissedActivityIdentities: [String: Date] = [:]
    var onDisposition: ((PetActivity, PetActivityDisposition) -> Void)?

    private static let maximumDismissedActivities = 256

    init() {}

    deinit {
        expirationTask?.cancel()
    }

    func startExpirationMonitoring() {
        guard expirationTask == nil else { return }
        expirationTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard let self, !Task.isCancelled else { return }
                if self.reducer.removeExpired() {
                    self.publishPresentation()
                }
            }
        }
    }

    func apply(_ event: PetActivityEvent) {
        if case let .upsert(activity) = event, shouldSuppress(activity) {
            return
        }
        let affectedSource: PetActivitySource?
        switch event {
        case let .upsert(activity):
            affectedSource = activity.source
        case let .remove(id):
            affectedSource = reducer.source(forActivityID: id) ?? Self.source(forActivityID: id)
        case let .removeSource(source):
            affectedSource = source
        case .reconcile:
            affectedSource = nil
        }
        reducer.apply(event)
        if let affectedSource {
            bumpVersion(for: affectedSource)
        }
        publishPresentation()
    }

    func replaceApprovals(_ approvals: [LocalConnectorPendingApproval]) {
        let pendingActivities = approvals.map { approval in
            PetActivity(
                id: "local-approval:\(approval.id)",
                source: .localApproval,
                kind: .waitingForApproval,
                title: "有本机操作等待审批",
                detail: approvalDetail(approval),
                updatedAt: Self.parseDate(approval.createdAt) ?? Date()
            )
        }
        let transientActivities = activities.filter {
            $0.source == .localApproval
                && $0.kind != .waitingForApproval
                && ($0.expiresAt.map { $0 > Date() } ?? false)
        }
        let activities = pendingActivities + transientActivities
        reducer.replace(source: .localApproval, with: activities)
        bumpVersion(for: .localApproval)
        publishPresentation()
    }

    func showApprovalEvent(_ event: LocalConnectorApprovalEvent) {
        let approved = event.decision.lowercased() == "approved"
        let title: String
        switch (event.reviewer, approved) {
        case (.ai, true): title = "AI 已批准本机操作"
        case (.ai, false): title = "AI 已拒绝本机操作"
        case (.policy, true): title = "无需审批，操作已放行"
        case (.policy, false): title = "操作已被策略拦截"
        case (.session, true): title = "操作已按会话授权放行"
        case (.session, false): title = "会话授权未通过"
        case (.user, true): title = "你已允许本机操作"
        case (.user, false): title = "你已拒绝本机操作"
        }
        let command = event.command.trimmingCharacters(in: .whitespacesAndNewlines)
        let reason = event.reason?.trimmingCharacters(in: .whitespacesAndNewlines)
        let detail = [command.isEmpty ? nil : command, reason?.isEmpty == false ? reason : nil]
            .compactMap { $0 }
            .joined(separator: "\n")
        apply(.upsert(PetActivity(
            id: "local-approval-event:\(event.id)",
            source: .localApproval,
            kind: approved ? .succeeded : .cancelled,
            title: title,
            detail: detail.isEmpty ? event.source : detail,
            eventID: event.id,
            updatedAt: event.createdAt,
            expiresAt: Date().addingTimeInterval(5)
        )))
    }

    func versions(for sources: [PetActivitySource]) -> [PetActivitySource: Int64] {
        Dictionary(uniqueKeysWithValues: sources.map { ($0, sourceVersions[$0, default: 0]) })
    }

    func reconcileActivities(
        _ activities: [PetActivity],
        sources: [PetActivitySource],
        expectedVersions: [PetActivitySource: Int64]
    ) {
        for source in sources {
            guard sourceVersions[source, default: 0] == expectedVersions[source, default: 0] else {
                continue
            }
            reducer.replace(
                source: source,
                with: activities.filter {
                    $0.source == source && !shouldSuppress($0)
                }
            )
            bumpVersion(for: source)
        }
        publishPresentation()
    }

    func clear() {
        reducer.removeAll()
        sourceVersions.removeAll()
        dismissedActivityIdentities.removeAll()
        publishPresentation()
    }

    func removeProcessActivities() {
        reducer.remove(kinds: [.working, .reviewing])
        bumpAllCloudVersions()
        publishPresentation()
    }

    func removeCompletionActivities() {
        reducer.remove(kinds: [.succeeded])
        bumpAllCloudVersions()
        publishPresentation()
    }

    func dismiss(
        _ activity: PetActivity,
        disposition: PetActivityDisposition = .ignored
    ) {
        dismissedActivityIdentities[activityIdentity(activity)] = Date()
        trimDismissedActivities()
        reducer.apply(.remove(id: activity.id))
        bumpVersion(for: activity.source)
        publishPresentation()
        onDisposition?(activity, disposition)
    }

    func restoreDismissal(_ activity: PetActivity) {
        dismissedActivityIdentities.removeValue(forKey: activityIdentity(activity))
    }

    private func publishPresentation() {
        let nextActivities = reducer.visibleActivities()
        let nextPresentation = reducer.presentation()
        if activities != nextActivities {
            activities = nextActivities
        }
        if presentation != nextPresentation {
            presentation = nextPresentation
        }
    }

    private func bumpVersion(for source: PetActivitySource) {
        sourceVersions[source, default: 0] &+= 1
    }

    private func bumpAllCloudVersions() {
        for source in Self.cloudSources {
            bumpVersion(for: source)
        }
    }

    private func shouldSuppress(_ activity: PetActivity) -> Bool {
        dismissedActivityIdentities[activityIdentity(activity)] != nil
    }

    private func activityIdentity(_ activity: PetActivity) -> String {
        let version = activity.activityVersion
            ?? activity.route.runID
            ?? activity.route.turnID
            ?? activity.eventID
            ?? "1"
        return "\(activity.source.rawValue)|\(activity.id)|\(version)"
    }

    private func trimDismissedActivities() {
        if dismissedActivityIdentities.count > Self.maximumDismissedActivities {
            let keep = dismissedActivityIdentities
                .sorted { $0.value > $1.value }
                .prefix(Self.maximumDismissedActivities)
            dismissedActivityIdentities = Dictionary(
                uniqueKeysWithValues: keep.map { ($0.key, $0.value) }
            )
        }
    }

    private static let cloudSources: [PetActivitySource] = [
        .askUserPrompt,
        .chat,
        .taskBoard,
        .taskRunner,
        .projectExecution,
    ]

    private static func source(forActivityID id: String) -> PetActivitySource? {
        if id.hasPrefix("ask-user:") { return .askUserPrompt }
        if id.hasPrefix("local-approval:") || id.hasPrefix("local-approval-event:") {
            return .localApproval
        }
        if id.hasPrefix("chat:") { return .chat }
        if id.hasPrefix("task-review:") || id.hasPrefix("task-board:") { return .taskBoard }
        if id.hasPrefix("task-runner:") { return .taskRunner }
        if id.hasPrefix("project-execution:") { return .projectExecution }
        return nil
    }

    private func approvalDetail(_ approval: LocalConnectorPendingApproval) -> String {
        let risk: String
        switch approval.risk.lowercased() {
        case "high", "critical": risk = "高风险"
        case "medium": risk = "中风险"
        default: risk = "低风险"
        }
        let command = approval.command.trimmingCharacters(in: .whitespacesAndNewlines)
        let metadata = "\(risk) · \(approval.source)"
        return command.isEmpty ? metadata : "\(command)\n\(metadata)"
    }

    private static func parseDate(_ value: String) -> Date? {
        ISO8601DateFormatter().date(from: value)
    }
}
