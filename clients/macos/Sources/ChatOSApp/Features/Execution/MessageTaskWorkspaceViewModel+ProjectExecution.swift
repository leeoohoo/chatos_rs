import ChatOSCore
import Foundation

extension MessageTaskWorkspaceViewModel {
    func confirmExecution() {
        guard let service = projectExecutionService,
              let identity = executionState.identity,
              executionState.canConfirm,
              !isMutatingPlan else {
            errorMessage = "任务图或执行标识尚未完整，当前不能安全启动执行。"
            return
        }
        isMutatingPlan = true
        errorMessage = nil
        planActionMessage = nil
        Task {
            do {
                _ = try await service.confirmExecution(identity)
                planActionMessage = "已确认执行，任务将按依赖顺序从起始节点运行。"
                markExecutionStarted()
                await refreshWorkspaceState(refreshInspector: true)
                startPollingIfNeeded(force: true)
            } catch {
                errorMessage = error.localizedDescription
            }
            isMutatingPlan = false
        }
    }

    func abandonPlan() {
        guard let service = projectExecutionService,
              let identity = executionState.identity,
              !isMutatingPlan else {
            errorMessage = "当前消息缺少完整执行标识，无法安全地放弃计划。"
            return
        }
        isMutatingPlan = true
        errorMessage = nil
        planActionMessage = nil
        Task {
            do {
                _ = try await service.abandonPlan(identity)
                isPlanStopped = true
                stopPolling()
                refresh()
            } catch {
                errorMessage = error.localizedDescription
            }
            isMutatingPlan = false
        }
    }
}
