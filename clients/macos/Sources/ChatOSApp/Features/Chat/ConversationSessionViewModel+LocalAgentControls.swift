import ChatOSCore
import Foundation

extension ConversationSessionViewModel {
    func runControls(for turnID: String) -> [LocalAgentRunControlState] {
        localAgentRunControls.filter { $0.turnID == turnID }
    }

    func toolApprovals(for turnID: String) -> [LocalAgentToolApprovalRequest] {
        localAgentToolApprovals.filter { $0.turnID == turnID }
    }

    func isOperatingOnRun(_ runID: String) -> Bool {
        localAgentControlOperationIDs.contains(runOperationID(runID))
    }

    func isOperatingOnToolApproval(_ invocationID: String) -> Bool {
        localAgentControlOperationIDs.contains(toolOperationID(invocationID))
    }

    func localAgentControlError(for id: String) -> String? {
        localAgentControlErrors[id]
    }

    func refreshLocalAgentControls() async {
        guard let localAgentRunControlService else { return }
        async let controls = localAgentRunControlService.fetchRunControls(sessionID: sessionID)
        async let approvals = localAgentRunControlService.fetchPendingToolApprovals(
            sessionID: sessionID
        )
        let (nextControls, nextApprovals) = await (controls, approvals)
        localAgentRunControls = nextControls
        localAgentToolApprovals = nextApprovals

        for (runID, initialStatus) in pendingLocalAgentRunStatuses {
            let currentStatus = nextControls.first(where: { $0.runID == runID })?.status
            if currentStatus == nil || currentStatus != initialStatus {
                pendingLocalAgentRunStatuses[runID] = nil
                localAgentControlOperationIDs.remove(runOperationID(runID))
            }
        }
        let pendingApprovalIDs = Set(nextApprovals.map(\.invocationID))
        let completedApprovalOperations = localAgentControlOperationIDs.filter {
            $0.hasPrefix("tool:") && !pendingApprovalIDs.contains(String($0.dropFirst(5)))
        }
        localAgentControlOperationIDs.subtract(completedApprovalOperations)
    }

    func pauseLocalAgentRun(_ control: LocalAgentRunControlState) {
        performRunControl(control) { service in
            try await service.pause(runID: control.runID, sessionID: self.sessionID)
        }
    }

    func resumeLocalAgentRun(_ control: LocalAgentRunControlState) {
        performRunControl(control) { service in
            try await service.resume(runID: control.runID, sessionID: self.sessionID)
        }
    }

    func cancelLocalAgentRun(_ control: LocalAgentRunControlState) {
        performRunControl(control) { service in
            try await service.cancel(runID: control.runID, sessionID: self.sessionID)
        }
    }

    func decideLocalAgentToolApproval(
        _ approval: LocalAgentToolApprovalRequest,
        decision: LocalAgentToolApprovalDecision
    ) {
        guard let localAgentRunControlService else { return }
        let operationID = toolOperationID(approval.invocationID)
        guard !localAgentControlOperationIDs.contains(operationID) else { return }
        localAgentControlOperationIDs.insert(operationID)
        localAgentControlErrors[approval.invocationID] = nil
        Task {
            do {
                let reason = decision == .approve
                    ? "用户允许本次工具执行"
                    : "用户拒绝本次工具执行"
                try await localAgentRunControlService.decideToolApproval(
                    invocationID: approval.invocationID,
                    sessionID: sessionID,
                    decision: decision,
                    reason: reason
                )
            } catch {
                localAgentControlOperationIDs.remove(operationID)
                localAgentControlErrors[approval.invocationID] = error.localizedDescription
                await refreshLocalAgentControls()
            }
        }
    }

    private func performRunControl(
        _ control: LocalAgentRunControlState,
        operation: @escaping (any LocalAgentRunControlServicing) async throws -> Void
    ) {
        guard let localAgentRunControlService else { return }
        let operationID = runOperationID(control.runID)
        guard !localAgentControlOperationIDs.contains(operationID) else { return }
        pendingLocalAgentRunStatuses[control.runID] = control.status
        localAgentControlOperationIDs.insert(operationID)
        localAgentControlErrors[control.runID] = nil
        Task {
            do {
                try await operation(localAgentRunControlService)
            } catch {
                pendingLocalAgentRunStatuses[control.runID] = nil
                localAgentControlOperationIDs.remove(operationID)
                localAgentControlErrors[control.runID] = error.localizedDescription
                await refreshLocalAgentControls()
            }
        }
    }

    private func runOperationID(_ runID: String) -> String { "run:\(runID)" }
    private func toolOperationID(_ invocationID: String) -> String { "tool:\(invocationID)" }
}
