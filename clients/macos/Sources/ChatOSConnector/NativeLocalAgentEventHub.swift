import ChatOSCore
import Foundation

/// Shares one Local Agent event cursor and long-poll loop across all UI consumers.
///
/// The Host can multiplex stdio requests, but independent event waiters still
/// duplicate SQLite reads without improving delivery latency. This hub keeps
/// one owner-scoped cursor and fans event pages out in process.
public actor NativeLocalAgentEventHub {
    public struct Update: Sendable {
        public enum Kind: Sendable {
            case reconcile
            case events([LocalAgentEventRecord])
        }

        public let ownerUserID: String
        public let kind: Kind

        fileprivate init(ownerUserID: String, kind: Kind) {
            self.ownerUserID = ownerUserID
            self.kind = kind
        }
    }

    private let client: NativeLocalAgentRuntimeClient
    private var ownerUserID: String?
    private var configurationGeneration: UInt64 = 0
    private var cursor: Int64?
    private var subscribers: [UUID: AsyncStream<Update>.Continuation] = [:]
    private var pollingTask: Task<Void, Never>?
    private var pollingToken: UUID?

    public init(host: any LocalAgentHostClientServicing) {
        self.client = NativeLocalAgentRuntimeClient(host: host)
    }

    public func configure(ownerUserID: String) {
        if self.ownerUserID != ownerUserID {
            configurationGeneration &+= 1
            pollingTask?.cancel()
            pollingTask = nil
            pollingToken = nil
            self.ownerUserID = ownerUserID
            cursor = nil
            broadcast(.init(ownerUserID: ownerUserID, kind: .reconcile))
        }
        startPollingIfNeeded()
    }

    public func reset() {
        configurationGeneration &+= 1
        ownerUserID = nil
        cursor = nil
        pollingTask?.cancel()
        pollingTask = nil
        pollingToken = nil
    }

    public func updates() -> AsyncStream<Update> {
        let subscriberID = UUID()
        // Host events are durable and every consumer reconciles from SQLite.
        // Bound wake-ups so a suspended window cannot accumulate an unlimited
        // process-local replay queue while the Host continues making progress.
        let pair = AsyncStream<Update>.makeStream(bufferingPolicy: .bufferingNewest(64))
        subscribers[subscriberID] = pair.continuation
        pair.continuation.onTermination = { [weak self] _ in
            Task { await self?.removeSubscriber(subscriberID) }
        }
        if let ownerUserID {
            pair.continuation.yield(.init(
                ownerUserID: ownerUserID,
                kind: .reconcile
            ))
        }
        startPollingIfNeeded()
        return pair.stream
    }

    private func removeSubscriber(_ subscriberID: UUID) {
        subscribers.removeValue(forKey: subscriberID)
        // Let the one in-flight Host wait finish naturally. Cancelling only the
        // Swift continuation cannot cancel work already dispatched inside the
        // Host and rapid view churn would otherwise accumulate abandoned waits.
        // The loop exits without issuing another request when this wait returns.
    }

    private func startPollingIfNeeded() {
        guard pollingTask == nil,
              ownerUserID != nil,
              !subscribers.isEmpty else { return }
        let token = UUID()
        pollingToken = token
        pollingTask = Task { [weak self] in
            await self?.poll(token: token)
        }
    }

    private func poll(token: UUID) async {
        defer { pollingDidEnd(token: token) }
        while !Task.isCancelled {
            guard pollingToken == token,
                  !subscribers.isEmpty,
                  let ownerUserID else { return }
            let generation = configurationGeneration
            do {
                if cursor == nil {
                    let latest = try await client.latestEventCursor(
                        ownerUserID: ownerUserID
                    )
                    guard isCurrent(
                        token: token,
                        ownerUserID: ownerUserID,
                        generation: generation
                    ) else { continue }
                    cursor = latest
                }

                let afterCursor = cursor ?? 0
                let page = try await client.waitEvents(
                    ownerUserID: ownerUserID,
                    afterCursor: afterCursor,
                    timeoutMilliseconds: NativeLocalAgentEventWaitPolicy.timeoutMilliseconds,
                    payloadMode: .routing
                )
                guard isCurrent(
                    token: token,
                    ownerUserID: ownerUserID,
                    generation: generation
                ) else { continue }

                cursor = max(afterCursor, page.nextCursor)
                if !page.events.isEmpty {
                    broadcast(.init(
                        ownerUserID: ownerUserID,
                        kind: .events(page.events)
                    ))
                }
            } catch is CancellationError {
                return
            } catch {
                guard isCurrent(
                    token: token,
                    ownerUserID: ownerUserID,
                    generation: generation
                ) else { continue }
                // Host restart and system wake can invalidate an in-flight stdio
                // request. Keep subscribers alive; lifecycle reconfiguration
                // resets the cursor and resumes this shared stream.
                do {
                    try await Task.sleep(for: .seconds(1))
                } catch {
                    return
                }
            }
        }
    }

    private func isCurrent(
        token: UUID,
        ownerUserID: String,
        generation: UInt64
    ) -> Bool {
        pollingToken == token
            && self.ownerUserID == ownerUserID
            && configurationGeneration == generation
            && !subscribers.isEmpty
    }

    private func pollingDidEnd(token: UUID) {
        guard pollingToken == token else { return }
        pollingTask = nil
        pollingToken = nil
        if subscribers.isEmpty {
            cursor = nil
        }
        startPollingIfNeeded()
    }

    private func broadcast(_ update: Update) {
        for continuation in subscribers.values {
            continuation.yield(update)
        }
    }
}
