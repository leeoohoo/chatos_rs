import ChatOSAPI
import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import AppKit
import Combine
import Foundation
import SwiftUI

@MainActor
extension AppModel {
    /// Runs independently of the Agent workspace UI. A due Agent gets one account-level wake-up;
    /// Relay reads its complete unread inbox and its durable TodoList in that single run.
    func restartAgentHeartbeatCoordinator() {
        agentHeartbeatTask?.cancel()
        agentCommunicationTask?.cancel()
        agentExecutorRecoveryTask?.cancel()
        guard let ownerUserID = authenticatedUserID else {
            agentHeartbeatTask = nil
            agentCommunicationTask = nil
            agentExecutorRecoveryTask = nil
            return
        }
        let service = agentGroupChatService
        let scheduler = agentGroupChatScheduler
        agentCommunicationTask = Task { [weak self] in
            let batchSize = 64
            let changes = await service.changes(ownerUserID: ownerUserID)
            let wakeups = AsyncStream<Void>(bufferingPolicy: .bufferingNewest(1)) { continuation in
                // Subscribe before the startup wake so a write racing with recovery remains buffered.
                continuation.yield()
                let changeTask = Task {
                    for await change in changes {
                        guard !Task.isCancelled else { break }
                        if AgentRuntimePollingPolicy.shouldWakeCommunicationRecovery(
                            for: change.kind
                        ) {
                            continuation.yield()
                        }
                    }
                }
                let fallbackTask = Task {
                    while !Task.isCancelled {
                        do {
                            try await Task.sleep(
                                for: AgentRuntimePollingPolicy.communicationRecoveryInterval
                            )
                        } catch {
                            break
                        }
                        guard !Task.isCancelled else { break }
                        continuation.yield()
                    }
                }
                continuation.onTermination = { _ in
                    changeTask.cancel()
                    fallbackTask.cancel()
                }
            }
            for await _ in wakeups {
                guard !Task.isCancelled, self?.authenticatedUserID == ownerUserID else { return }
                var shouldContinue = true
                while shouldContinue, !Task.isCancelled {
                    do {
                        let results = try await scheduler.drainCommunications(
                            ownerUserID: ownerUserID,
                            maximumRuns: batchSize
                        )
                        // A full batch may leave more durable work behind; drain it without waiting
                        // for another event. An empty/partial batch returns to the wake stream.
                        shouldContinue = results.count == batchSize
                    } catch is CancellationError {
                        return
                    } catch {
                        guard !Task.isCancelled else { return }
                        try? await Task.sleep(for: .seconds(10))
                    }
                }
            }
        }
        agentExecutorRecoveryTask = Task { [weak self] in
            let changes = await service.changes(ownerUserID: ownerUserID)
            let wakeups = AsyncStream<Void>(bufferingPolicy: .bufferingNewest(1)) { continuation in
                continuation.yield()
                let changeTask = Task {
                    for await change in changes {
                        guard !Task.isCancelled else { break }
                        if AgentRuntimePollingPolicy.shouldWakeExecutorRecovery(
                            for: change.kind
                        ) {
                            continuation.yield()
                        }
                    }
                }
                let fallbackTask = Task {
                    while !Task.isCancelled {
                        do {
                            try await Task.sleep(
                                for: AgentRuntimePollingPolicy.executorRecoveryInterval
                            )
                        } catch {
                            break
                        }
                        continuation.yield()
                    }
                }
                continuation.onTermination = { _ in
                    changeTask.cancel()
                    fallbackTask.cancel()
                }
            }
            for await _ in wakeups {
                guard !Task.isCancelled, self?.authenticatedUserID == ownerUserID else { return }
                do {
                    _ = try await scheduler.drainAccount(ownerUserID: ownerUserID)
                } catch is CancellationError {
                    return
                } catch {
                    guard !Task.isCancelled else { return }
                }
            }
        }
        agentHeartbeatTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    let store = try await service.store()
                    let now = Int64(Date().timeIntervalSince1970 * 1_000)
                    let deliveries = try await store.enqueueDueAgentHeartbeats(
                        ownerUserID: ownerUserID,
                        nowUnixMs: now
                    )
                    // Heartbeats are manager-lane work. Publish a process-local invalidation so
                    // the durable account coordinator drains them without making this deadline
                    // loop wait behind a long executor Run.
                    for roomID in Set(deliveries.map(\.roomID)) {
                        await service.publishChange(.init(
                            ownerUserID: ownerUserID,
                            roomID: roomID,
                            kind: .roomUpdated
                        ))
                    }
                    guard !Task.isCancelled, self?.authenticatedUserID == ownerUserID else {
                        return
                    }
                    let nextDue = try await store.nextAgentHeartbeatDue(
                        ownerUserID: ownerUserID
                    )
                    let delayReferenceTime = Int64(Date().timeIntervalSince1970 * 1_000)
                    let delayMilliseconds = AgentRuntimePollingPolicy.heartbeatDelayMilliseconds(
                        nextDueUnixMs: nextDue,
                        nowUnixMs: delayReferenceTime
                    )
                    try await Task.sleep(for: .milliseconds(delayMilliseconds))
                } catch is CancellationError {
                    return
                } catch {
                    guard !Task.isCancelled else { return }
                    try? await Task.sleep(for: .seconds(10))
                }
            }
        }
    }

    func ensureAgentArtifactStorageCoordinator() {
        startAgentArtifactStorageCoordinator(forceRestart: false)
    }

    func restartAgentArtifactStorageCoordinator() {
        startAgentArtifactStorageCoordinator(forceRestart: true)
    }

    private func startAgentArtifactStorageCoordinator(forceRestart: Bool) {
        guard let ownerUserID = authenticatedUserID else {
            agentArtifactStorageTask?.cancel()
            agentArtifactStorageTask = nil
            agentArtifactStorageOwnerUserID = nil
            return
        }
        let hasLiveTask = agentArtifactStorageTask.map { !$0.isCancelled } ?? false
        guard AgentArtifactStorageCoordinatorPolicy.shouldStart(
            existingOwnerUserID: agentArtifactStorageOwnerUserID,
            requestedOwnerUserID: ownerUserID,
            hasLiveTask: hasLiveTask,
            forceRestart: forceRestart
        ) else { return }

        agentArtifactStorageTask?.cancel()
        agentArtifactStorageOwnerUserID = ownerUserID
        let service = agentGroupChatService
        agentArtifactStorageTask = Task { [weak self] in
            let changes = await service.changes(ownerUserID: ownerUserID)
            let wakeups = AsyncStream<Void>.makeStream(
                bufferingPolicy: .bufferingNewest(1)
            )
            let changeTask = Task {
                for await change in changes {
                    guard !Task.isCancelled else { return }
                    if AgentRuntimePollingPolicy.shouldWakeArtifactStorage(for: change.kind) {
                        wakeups.continuation.yield()
                    }
                }
            }
            var timerTask: Task<Void, Never>?
            defer {
                changeTask.cancel()
                timerTask?.cancel()
                wakeups.continuation.finish()
            }
            wakeups.continuation.yield()
            for await _ in wakeups.stream {
                guard !Task.isCancelled else { return }
                timerTask?.cancel()
                timerTask = nil
                do {
                    _ = try await service.persistPendingAgentArtifacts(ownerUserID: ownerUserID)
                    guard !Task.isCancelled, self?.authenticatedUserID == ownerUserID else {
                        return
                    }
                    let store = try await service.store()
                    let nextDue = try await store.nextAgentArtifactStorageDue(ownerUserID: ownerUserID)
                    let now = Int64(Date().timeIntervalSince1970 * 1_000)
                    let delayMilliseconds = AgentRuntimePollingPolicy.artifactStorageDelayMilliseconds(
                        nextDueUnixMs: nextDue,
                        nowUnixMs: now
                    )
                    timerTask = Task {
                        do {
                            try await Task.sleep(for: .milliseconds(delayMilliseconds))
                        } catch {
                            return
                        }
                        guard !Task.isCancelled else { return }
                        wakeups.continuation.yield()
                    }
                } catch is CancellationError {
                    return
                } catch {
                    guard !Task.isCancelled else { return }
                    timerTask = Task {
                        do {
                            try await Task.sleep(for: .seconds(10))
                        } catch {
                            return
                        }
                        guard !Task.isCancelled else { return }
                        wakeups.continuation.yield()
                    }
                }
            }
        }
    }

}

enum AgentArtifactStorageCoordinatorPolicy {
    static func shouldStart(
        existingOwnerUserID: String?,
        requestedOwnerUserID: String,
        hasLiveTask: Bool,
        forceRestart: Bool
    ) -> Bool {
        forceRestart
            || !hasLiveTask
            || existingOwnerUserID != requestedOwnerUserID
    }
}

enum AgentRuntimePollingPolicy {
    static let minimumHeartbeatDelayMilliseconds: Int64 = 1_000
    static let maximumHeartbeatDelayMilliseconds: Int64 = 300_000
    static let idleHeartbeatDelayMilliseconds: Int64 = 1_800_000
    static let communicationRecoveryInterval: Duration = .seconds(30)
    static let executorRecoveryInterval: Duration = .seconds(30)
    static let minimumArtifactStorageDelayMilliseconds: Int64 = 1_000
    static let maximumArtifactStorageDelayMilliseconds: Int64 = 300_000
    static let idleArtifactStorageDelayMilliseconds: Int64 = 1_800_000

    static func heartbeatDelayMilliseconds(
        nextDueUnixMs: Int64?,
        nowUnixMs: Int64
    ) -> Int64 {
        // Profile saves, authentication changes and system wake all restart the coordinator.
        // With no scheduled heartbeat, this timer is only a crash-recovery safety net.
        guard let nextDueUnixMs else { return idleHeartbeatDelayMilliseconds }
        return min(
            maximumHeartbeatDelayMilliseconds,
            max(minimumHeartbeatDelayMilliseconds, nextDueUnixMs - nowUnixMs)
        )
    }

    static func artifactStorageDelayMilliseconds(
        nextDueUnixMs: Int64?,
        nowUnixMs: Int64
    ) -> Int64 {
        // Room/run changes wake storage immediately; nil means this is only a recovery sweep.
        guard let nextDueUnixMs else { return idleArtifactStorageDelayMilliseconds }
        return min(
            maximumArtifactStorageDelayMilliseconds,
            max(minimumArtifactStorageDelayMilliseconds, nextDueUnixMs - nowUnixMs)
        )
    }

    static func shouldWakeArtifactStorage(
        for changeKind: NativeAgentGroupChatChange.Kind
    ) -> Bool {
        switch changeKind {
        case .roomUpdated: true
        case .deliveryClaimed, .runUpdated: false
        }
    }

    static func shouldWakeCommunicationRecovery(
        for changeKind: NativeAgentGroupChatChange.Kind
    ) -> Bool {
        switch changeKind {
        case .roomUpdated: true
        // Model/tool checkpoints do not create communication work. Chat and Todo mutations
        // publish roomUpdated, while startup and the fallback timer reconcile missed signals.
        case .deliveryClaimed, .runUpdated: false
        }
    }

    static func shouldWakeExecutorRecovery(
        for changeKind: NativeAgentGroupChatChange.Kind
    ) -> Bool {
        switch changeKind {
        case .roomUpdated: true
        // A running Agent persists several checkpoints per model/tool turn. Those updates belong
        // to the live owner in this process and must not launch an interrupted-Run scan. Queue
        // mutations publish roomUpdated; startup and the fallback timer cover crash recovery.
        case .deliveryClaimed, .runUpdated: false
        }
    }
}
