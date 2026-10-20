import Foundation

/// Identity for one Host liveness probe.
///
/// Health checks are account and lifecycle scoped. A response from an older account session or
/// an older Host process must never restart the current Host.
public struct LocalAgentHostHealthCheckToken: Sendable, Equatable {
    public let ownerUserID: String
    public let accountGeneration: UInt64
    public let lifecycleGeneration: UInt64
    public let requestID: UUID

    public init(
        ownerUserID: String,
        accountGeneration: UInt64,
        lifecycleGeneration: UInt64,
        requestID: UUID
    ) {
        self.ownerUserID = ownerUserID
        self.accountGeneration = accountGeneration
        self.lifecycleGeneration = lifecycleGeneration
        self.requestID = requestID
    }
}

/// Identity for one delayed Host restart.
public struct LocalAgentHostRestartToken: Sendable, Equatable {
    public let ownerUserID: String
    public let accountGeneration: UInt64
    public let lifecycleGeneration: UInt64
    public let attempt: Int
    public let requestID: UUID

    public init(
        ownerUserID: String,
        accountGeneration: UInt64,
        lifecycleGeneration: UInt64,
        attempt: Int,
        requestID: UUID
    ) {
        self.ownerUserID = ownerUserID
        self.accountGeneration = accountGeneration
        self.lifecycleGeneration = lifecycleGeneration
        self.attempt = attempt
        self.requestID = requestID
    }
}

public struct LocalAgentHostRestartPlan: Sendable, Equatable {
    public let token: LocalAgentHostRestartToken
    public let delay: Duration

    public init(token: LocalAgentHostRestartToken, delay: Duration) {
        self.token = token
        self.delay = delay
    }
}

/// Application-owned coordination for Host health probes and delayed crash recovery.
///
/// Cancellation remains an optimization. Every result is also checked against a typed token so a
/// delayed probe or retry cannot affect a newer account session or Host lifecycle, even when the
/// underlying async operation ignores cancellation.
public struct LocalAgentHostRecoveryState: Sendable, Equatable {
    public private(set) var activeHealthCheck: LocalAgentHostHealthCheckToken?
    public private(set) var activeRestart: LocalAgentHostRestartToken?
    public private(set) var recoveryAttempts = 0

    private static let restartDelays: [Duration] = [
        .milliseconds(250),
        .seconds(1),
        .seconds(2),
        .seconds(5),
        .seconds(10),
    ]

    public init() {}

    public var hasExhaustedRestarts: Bool {
        recoveryAttempts >= Self.restartDelays.count
    }

    public mutating func beginHealthCheck(
        ownerUserID: String,
        accountGeneration: UInt64,
        lifecycleGeneration: UInt64
    ) -> LocalAgentHostHealthCheckToken {
        let token = LocalAgentHostHealthCheckToken(
            ownerUserID: ownerUserID,
            accountGeneration: accountGeneration,
            lifecycleGeneration: lifecycleGeneration,
            requestID: UUID()
        )
        activeHealthCheck = token
        return token
    }

    public func acceptsHealthCheck(
        _ token: LocalAgentHostHealthCheckToken,
        ownerUserID: String?,
        accountGeneration: UInt64,
        lifecycleGeneration: UInt64
    ) -> Bool {
        activeHealthCheck == token
            && token.ownerUserID == ownerUserID
            && token.accountGeneration == accountGeneration
            && token.lifecycleGeneration == lifecycleGeneration
    }

    @discardableResult
    public mutating func finishHealthCheck(_ token: LocalAgentHostHealthCheckToken) -> Bool {
        guard activeHealthCheck == token else { return false }
        activeHealthCheck = nil
        return true
    }

    public mutating func cancelHealthCheck() {
        activeHealthCheck = nil
    }

    public mutating func scheduleRestart(
        ownerUserID: String,
        accountGeneration: UInt64,
        lifecycleGeneration: UInt64
    ) -> LocalAgentHostRestartPlan? {
        guard activeRestart == nil,
              !hasExhaustedRestarts else { return nil }
        recoveryAttempts += 1
        let token = LocalAgentHostRestartToken(
            ownerUserID: ownerUserID,
            accountGeneration: accountGeneration,
            lifecycleGeneration: lifecycleGeneration,
            attempt: recoveryAttempts,
            requestID: UUID()
        )
        activeRestart = token
        return LocalAgentHostRestartPlan(
            token: token,
            delay: Self.restartDelays[recoveryAttempts - 1]
        )
    }

    public func acceptsRestart(
        _ token: LocalAgentHostRestartToken,
        ownerUserID: String?,
        accountGeneration: UInt64,
        lifecycleGeneration: UInt64
    ) -> Bool {
        activeRestart == token
            && token.ownerUserID == ownerUserID
            && token.accountGeneration == accountGeneration
            && token.lifecycleGeneration == lifecycleGeneration
    }

    @discardableResult
    public mutating func finishRestart(_ token: LocalAgentHostRestartToken) -> Bool {
        guard activeRestart == token else { return false }
        activeRestart = nil
        return true
    }

    public mutating func resetRestarts() {
        activeRestart = nil
        recoveryAttempts = 0
    }

    public mutating func cancelRestart() {
        activeRestart = nil
    }

    public mutating func reset() {
        activeHealthCheck = nil
        resetRestarts()
    }
}
