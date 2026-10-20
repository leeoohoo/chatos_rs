import ChatOSConnector
import ChatOSCore
import Foundation

@MainActor
extension AgentGroupChatViewModel {
    struct RunRefreshRequest {
        let runID: UUID
        let visibleRoomID: String
        let ownerUserID: String
        let openedStore: SQLiteAgentGroupChatStore?
        let service: NativeAgentGroupChatService
    }

    struct RunRefreshOutcome {
        let store: SQLiteAgentGroupChatStore?
        let run: LocalAgentGroupChatRun?
        let delivery: ProjectAgentDelivery?
        let todoPresentation: TeamTodoRunPresentation?
        let errorMessage: String?
    }

    func cancelScheduledRunRefreshes() {
        for task in runRefreshTasks.values { task.cancel() }
        runRefreshTasks.removeAll()
    }

    /// Active runs persist many checkpoints per second. Coalesce each Run independently so a
    /// checkpoint updates one row instead of re-decoding the entire room history.
    func scheduleRunRefresh(_ runID: UUID) {
        runRefreshTasks[runID]?.cancel()
        runRefreshTasks[runID] = Task { [weak self] in
            do {
                try await Task.sleep(for: .milliseconds(240))
            } catch {
                return
            }
            guard let request = self?.makeRunRefreshRequest(runID) else { return }
            let outcome = await Self.fetchRunRefresh(request)
            guard !Task.isCancelled else { return }
            self?.applyRunRefresh(outcome, request: request)
            self?.runRefreshTasks.removeValue(forKey: runID)
        }
    }

    private func makeRunRefreshRequest(_ runID: UUID) -> RunRefreshRequest? {
        guard let visibleRoomID = room?.id else { return nil }
        return RunRefreshRequest(
            runID: runID,
            visibleRoomID: visibleRoomID,
            ownerUserID: ownerUserID,
            openedStore: openedStore,
            service: service
        )
    }

    nonisolated static func fetchRunRefresh(
        _ request: RunRefreshRequest
    ) async -> RunRefreshOutcome {
        do {
            let store: SQLiteAgentGroupChatStore
            if let openedStore = request.openedStore {
                store = openedStore
            } else {
                store = try await request.service.store()
            }
            guard let run = try await store.run(
                    ownerUserID: request.ownerUserID,
                    runID: request.runID
                  ), run.context.roomID == request.visibleRoomID,
                  let delivery = try await store.delivery(
                    ownerUserID: request.ownerUserID,
                    deliveryID: run.context.deliveryID
                  ), delivery.roomID == request.visibleRoomID else {
                return RunRefreshOutcome(
                    store: store,
                    run: nil,
                    delivery: nil,
                    todoPresentation: nil,
                    errorMessage: nil
                )
            }
            let presentation: TeamTodoRunPresentation?
            if delivery.todoID != nil {
                presentation = try? await AppCancellableDetachedWork.run(
                    priority: .utility,
                    operation: { TeamTodoRunPresentation(run: run) }
                )
            } else {
                presentation = nil
            }
            return RunRefreshOutcome(
                store: store,
                run: run,
                delivery: delivery,
                todoPresentation: presentation,
                errorMessage: nil
            )
        } catch {
            return RunRefreshOutcome(
                store: nil,
                run: nil,
                delivery: nil,
                todoPresentation: nil,
                errorMessage: error.localizedDescription
            )
        }
    }

    private func applyRunRefresh(
        _ outcome: RunRefreshOutcome,
        request: RunRefreshRequest
    ) {
        guard room?.id == request.visibleRoomID else { return }
        if openedStore == nil, let store = outcome.store {
            openedStore = store
        }
        if let errorMessage = outcome.errorMessage {
            self.errorMessage = errorMessage
            return
        }
        guard let run = outcome.run, let delivery = outcome.delivery else { return }

        if let index = recentRuns.firstIndex(where: { $0.id == run.id }) {
            recentRuns[index] = run
        } else {
            recentRuns.append(run)
        }
        recentRuns.sort {
            if $0.updatedAtUnixMs != $1.updatedAtUnixMs {
                return $0.updatedAtUnixMs > $1.updatedAtUnixMs
            }
            return $0.id.uuidString > $1.id.uuidString
        }
        if recentRuns.count > 500 {
            let removedIDs = Set(recentRuns.dropFirst(500).map(\.id))
            recentRuns.removeSubrange(500...)
            for removedID in removedIDs {
                recentRunDeliveries.removeValue(forKey: removedID)
            }
        }
        recentRunDeliveries[run.id] = delivery

        if hasLoadedRunHistory {
            let summary = LocalAgentRunHistorySummary(
                run: run,
                triggerKind: delivery.triggerKind
            )
            recentRunHistorySummaries.removeAll { $0.id == summary.id }
            recentRunHistorySummaries.append(summary)
            recentRunHistorySummaries.sort {
                if $0.updatedAtUnixMs != $1.updatedAtUnixMs {
                    return $0.updatedAtUnixMs > $1.updatedAtUnixMs
                }
                return $0.id.uuidString > $1.id.uuidString
            }
            if recentRunHistorySummaries.count > 500 {
                recentRunHistorySummaries.removeSubrange(500...)
            }
        }

        updateInterruptedRun(run, delivery: delivery)
        if let todoID = Self.todoID(for: delivery) {
            let latestRunID = recentRuns.first { candidate in
                recentRunDeliveries[candidate.id].flatMap { Self.todoID(for: $0) } == todoID
            }?.id
            if latestRunID == run.id, let presentation = outcome.todoPresentation {
                todoRunPresentationsByTodoID[todoID] = presentation
            }
        }
    }

    private func updateInterruptedRun(
        _ run: LocalAgentGroupChatRun,
        delivery: ProjectAgentDelivery
    ) {
        interruptedRuns.removeAll { $0.run.id == run.id }
        guard run.checkpoint.status != .completed,
              run.checkpoint.status != .failed,
              delivery.status == .running else { return }
        interruptedRuns.append(.init(
            run: run,
            delivery: delivery,
            agentName: profilesByID[run.context.agentID]?.draft.name ?? run.context.agentID
        ))
        interruptedRuns.sort { $0.run.updatedAtUnixMs > $1.run.updatedAtUnixMs }
    }

    private static func todoID(for delivery: ProjectAgentDelivery) -> String? {
        delivery.todoID
    }
}
