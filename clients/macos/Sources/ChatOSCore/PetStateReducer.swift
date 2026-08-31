import Foundation

public struct PetStateReducer: Sendable {
    private var activities: [String: PetActivity] = [:]
    private var recentEventIDs: [String] = []
    private var recentEventIDSet: Set<String> = []
    private let maximumRememberedEventIDs: Int

    public init(maximumRememberedEventIDs: Int = 256) {
        self.maximumRememberedEventIDs = max(32, maximumRememberedEventIDs)
    }

    public mutating func apply(_ event: PetActivityEvent) {
        switch event {
        case let .upsert(activity):
            guard rememberEventIDIfNeeded(activity.eventID) else { return }
            if let current = activities[activity.id],
               let currentSequence = current.eventSequence,
               let nextSequence = activity.eventSequence,
               nextSequence < currentSequence {
                return
            }
            activities[activity.id] = activity
        case let .remove(id):
            activities.removeValue(forKey: id)
        case let .removeSource(source):
            activities = activities.filter { $0.value.source != source }
        case .reconcile:
            break
        }
    }

    public mutating func replace(
        source: PetActivitySource,
        with nextActivities: [PetActivity]
    ) {
        activities = activities.filter { $0.value.source != source }
        for activity in nextActivities {
            activities[activity.id] = activity
        }
    }

    @discardableResult
    public mutating func removeExpired(at date: Date = Date()) -> Bool {
        let previousCount = activities.count
        activities = activities.filter { _, activity in
            guard let expiresAt = activity.expiresAt else { return true }
            return expiresAt > date
        }
        return activities.count != previousCount
    }

    public mutating func removeAll() {
        activities.removeAll()
    }

    public mutating func remove(kinds: [PetActivityKind]) {
        activities = activities.filter { !kinds.contains($0.value.kind) }
    }

    public func source(forActivityID id: String) -> PetActivitySource? {
        activities[id]?.source
    }

    public func visibleActivities(at date: Date = Date()) -> [PetActivity] {
        let candidates = activities.values
            .filter { activity in
                activity.expiresAt.map { $0 > date } ?? true
            }
        let conversationsWithSpecificWork = Set(candidates.compactMap { activity -> String? in
            guard activity.kind == .working || activity.kind == .reviewing,
                  activity.source == .taskRunner || activity.source == .taskBoard else {
                return nil
            }
            return activity.route.conversationID
        })
        return candidates
            .filter { activity in
                guard activity.kind == .working || activity.kind == .reviewing,
                      activity.source == .chat || activity.source == .projectExecution,
                      let conversationID = activity.route.conversationID else {
                    return true
                }
                return !conversationsWithSpecificWork.contains(conversationID)
            }
            .sorted { lhs, rhs in
                let lhsPriority = presentationPriority(for: lhs)
                let rhsPriority = presentationPriority(for: rhs)
                if lhsPriority != rhsPriority {
                    return lhsPriority > rhsPriority
                }
                if lhs.updatedAt != rhs.updatedAt {
                    return lhs.updatedAt > rhs.updatedAt
                }
                return lhs.id > rhs.id
            }
    }

    private func presentationPriority(for activity: PetActivity) -> Int {
        if activity.source == .localApproval,
           activity.kind != .waitingForApproval,
           activity.expiresAt != nil {
            return 550
        }
        return activity.kind.presentationPriority
    }

    public func presentation(at date: Date = Date()) -> PetPresentation {
        let visible = visibleActivities(at: date)
        guard let primary = visible.first else {
            return .idle
        }
        return PetPresentation(
            animationState: primary.kind.animationState,
            primaryActivity: primary,
            activeWorkCount: visible.filter {
                $0.kind == .working || $0.kind == .reviewing
            }.count,
            attentionCount: visible.filter {
                $0.kind == .waitingForApproval || $0.kind == .waitingForUser
            }.count
        )
    }

    private mutating func rememberEventIDIfNeeded(_ eventID: String?) -> Bool {
        guard let eventID, !eventID.isEmpty else { return true }
        guard recentEventIDSet.insert(eventID).inserted else { return false }
        recentEventIDs.append(eventID)
        if recentEventIDs.count > maximumRememberedEventIDs {
            let overflow = recentEventIDs.count - maximumRememberedEventIDs
            for removed in recentEventIDs.prefix(overflow) {
                recentEventIDSet.remove(removed)
            }
            recentEventIDs.removeFirst(overflow)
        }
        return true
    }
}
