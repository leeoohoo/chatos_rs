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
        guard let ownerUserID = authenticatedUserID else {
            agentHeartbeatTask = nil
            agentCommunicationTask = nil
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
                        switch change.kind {
                        case .roomUpdated, .runUpdated:
                            continuation.yield()
                        case .deliveryClaimed:
                            break
                        }
                    }
                }
                let fallbackTask = Task {
                    while !Task.isCancelled {
                        do {
                            try await Task.sleep(for: .seconds(300))
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
        agentHeartbeatTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    let store = try await service.store()
                    let now = Int64(Date().timeIntervalSince1970 * 1_000)
                    _ = try await store.enqueueDueAgentHeartbeats(
                        ownerUserID: ownerUserID,
                        nowUnixMs: now
                    )
                    // Communication has its own coordinator above. This pass recovers and drains
                    // durable executor work left pending by an app crash after enqueue.
                    _ = try await scheduler.drainAccount(ownerUserID: ownerUserID)
                    guard !Task.isCancelled, self?.authenticatedUserID == ownerUserID else {
                        return
                    }
                    let nextDue = try await store.nextAgentHeartbeatDue(
                        ownerUserID: ownerUserID
                    )
                    let delayMilliseconds: Int64
                    if let nextDue {
                        delayMilliseconds = min(60_000, max(1_000, nextDue - now))
                    } else {
                        delayMilliseconds = 60_000
                    }
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

    func restartAgentArtifactSyncCoordinator() {
        agentArtifactSyncTask?.cancel()
        guard let ownerUserID = authenticatedUserID else {
            agentArtifactSyncTask = nil
            return
        }
        let service = agentGroupChatService
        agentArtifactSyncTask = Task { [weak self] in
            while !Task.isCancelled {
                do {
                    _ = try await service.syncPendingAgentArtifacts(ownerUserID: ownerUserID)
                    guard !Task.isCancelled, self?.authenticatedUserID == ownerUserID else {
                        return
                    }
                    let store = try await service.store()
                    let nextDue = try await store.nextAgentArtifactSyncDue(ownerUserID: ownerUserID)
                    let now = Int64(Date().timeIntervalSince1970 * 1_000)
                    let delayMilliseconds = nextDue.map {
                        min(Int64(60_000), max(Int64(1_000), $0 - now))
                    } ?? 60_000
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

}
