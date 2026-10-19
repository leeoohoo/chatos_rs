import Foundation

public enum ConversationHistoryRefreshPresentation: Sendable, Equatable {
    case silent
    case visible

    public func merged(with other: Self) -> Self {
        self == .visible || other == .visible ? .visible : .silent
    }
}

/// Pure coordination state for latest-history refresh debounce and in-flight coalescing.
/// Task creation, cancellation, and UI loading presentation remain client responsibilities.
public struct ConversationLatestRefreshState: Sendable, Equatable {
    public static let debounceDelay: Duration = .milliseconds(250)

    public private(set) var scheduledPresentation: ConversationHistoryRefreshPresentation?
    public private(set) var isInFlight = false
    public private(set) var pendingPresentation: ConversationHistoryRefreshPresentation?

    public init() {}

    public mutating func scheduleDebounced(
        _ presentation: ConversationHistoryRefreshPresentation
    ) -> ConversationHistoryRefreshPresentation {
        let scheduled = scheduledPresentation?.merged(with: presentation) ?? presentation
        scheduledPresentation = scheduled
        return scheduled
    }

    public mutating func consumeDebounced() {
        scheduledPresentation = nil
    }

    public mutating func cancelScheduledAndPending() {
        scheduledPresentation = nil
        pendingPresentation = nil
    }

    public mutating func beginOrQueue(
        _ presentation: ConversationHistoryRefreshPresentation
    ) -> Bool {
        guard !isInFlight else {
            pendingPresentation = pendingPresentation?.merged(with: presentation)
                ?? presentation
            return false
        }
        isInFlight = true
        pendingPresentation = nil
        return true
    }

    public mutating func complete() -> ConversationHistoryRefreshPresentation? {
        isInFlight = false
        defer { pendingPresentation = nil }
        return pendingPresentation
    }
}
