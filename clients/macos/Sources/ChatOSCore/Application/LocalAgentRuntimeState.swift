import Foundation

/// Application-owned lifecycle state for the authenticated Local Agent runtime.
///
/// Process existence is deliberately not represented as readiness. A runtime becomes ready only
/// after the Host handshake, workspace synchronization, control-plane restoration, and service
/// configuration have completed for the same account and lifecycle generation.
public struct LocalAgentRuntimeState: Sendable, Equatable {
    public enum Phase: String, Sendable, Equatable {
        case stopped
        case launching
        case handshaking
        case synchronizing
        case ready
        case degraded
        case restarting
    }

    public private(set) var phase: Phase
    public private(set) var ownerUserID: String?
    public private(set) var generation: UInt64
    public private(set) var recoveryAttempt: Int
    public private(set) var errorMessage: String?

    public init(
        phase: Phase = .stopped,
        ownerUserID: String? = nil,
        generation: UInt64 = 0,
        recoveryAttempt: Int = 0,
        errorMessage: String? = nil
    ) {
        self.phase = phase
        self.ownerUserID = ownerUserID
        self.generation = generation
        self.recoveryAttempt = recoveryAttempt
        self.errorMessage = errorMessage
    }

    public func isReady(for ownerUserID: String?) -> Bool {
        guard let ownerUserID else { return false }
        return phase == .ready && self.ownerUserID == ownerUserID
    }

    public mutating func beginLaunch(ownerUserID: String, generation: UInt64) {
        guard generation >= self.generation else { return }
        self = .init(
            phase: .launching,
            ownerUserID: ownerUserID,
            generation: generation
        )
    }

    public mutating func beginHandshake(ownerUserID: String, generation: UInt64) {
        transition(
            to: .handshaking,
            ownerUserID: ownerUserID,
            generation: generation
        )
    }

    public mutating func beginSynchronization(ownerUserID: String, generation: UInt64) {
        transition(
            to: .synchronizing,
            ownerUserID: ownerUserID,
            generation: generation
        )
    }

    public mutating func becomeReady(ownerUserID: String, generation: UInt64) {
        transition(
            to: .ready,
            ownerUserID: ownerUserID,
            generation: generation
        )
    }

    public mutating func becomeDegraded(
        ownerUserID: String?,
        generation: UInt64,
        errorMessage: String
    ) {
        guard generation == self.generation,
              self.ownerUserID == ownerUserID || self.ownerUserID == nil else { return }
        phase = .degraded
        self.ownerUserID = ownerUserID
        recoveryAttempt = 0
        self.errorMessage = errorMessage
    }

    public mutating func scheduleRestart(
        ownerUserID: String,
        generation: UInt64,
        attempt: Int,
        errorMessage: String? = nil
    ) {
        guard generation == self.generation,
              self.ownerUserID == ownerUserID || self.ownerUserID == nil else { return }
        phase = .restarting
        self.ownerUserID = ownerUserID
        recoveryAttempt = max(1, attempt)
        if let errorMessage {
            self.errorMessage = errorMessage
        }
    }

    public mutating func stop(generation: UInt64) {
        guard generation >= self.generation else { return }
        self = .init(phase: .stopped, generation: generation)
    }

    private mutating func transition(
        to phase: Phase,
        ownerUserID: String,
        generation: UInt64
    ) {
        guard generation == self.generation,
              self.ownerUserID == ownerUserID else { return }
        self.phase = phase
        recoveryAttempt = 0
        errorMessage = nil
    }
}
