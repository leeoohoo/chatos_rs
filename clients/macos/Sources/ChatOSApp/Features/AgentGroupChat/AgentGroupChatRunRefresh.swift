import ChatOSConnector
import ChatOSCore
import Foundation

@MainActor
extension AgentGroupChatViewModel {
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
            guard let self else { return }
            await refreshRun(runID)
            runRefreshTasks.removeValue(forKey: runID)
        }
    }

    private func refreshRun(_ runID: UUID) async {
        guard let visibleRoomID = room?.id else { return }
        do {
            let store = try await resolveStore()
            guard let run = try await store.run(ownerUserID: ownerUserID, runID: runID),
                  run.context.roomID == visibleRoomID,
                  let delivery = try await store.delivery(
                    ownerUserID: ownerUserID,
                    deliveryID: run.context.deliveryID
                  ), delivery.roomID == visibleRoomID,
                  room?.id == visibleRoomID else { return }

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
                if latestRunID == run.id {
                    guard let presentation = try? await AppCancellableDetachedWork.run(
                        priority: .utility,
                        operation: {
                        TeamTodoRunPresentation(run: run)
                        }
                    ) else { return }
                    guard room?.id == visibleRoomID else { return }
                    todoRunPresentationsByTodoID[todoID] = presentation
                }
            }
        } catch {
            guard room?.id == visibleRoomID else { return }
            errorMessage = error.localizedDescription
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
        guard delivery.triggerKind == .todo,
              delivery.deduplicationKey.hasPrefix("todo:") else { return nil }
        return String(delivery.deduplicationKey.dropFirst("todo:".count))
    }
}
