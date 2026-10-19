import ChatOSCore
import Foundation

enum MessageTaskRunActivityPresentation {
    static func title(
        for kind: MessageTaskRunActivityKind,
        localized: (String, String) -> String
    ) -> String {
        switch kind {
        case .waitingDependency: localized("等待前置任务", "Waiting for Dependency")
        case .waitingModel: localized("等待模型响应", "Waiting for Model")
        case .runningModel: localized("模型正在规划", "Model Is Working")
        case .waitingTool: localized("等待工具返回", "Waiting for Tool")
        case .waitingApproval: localized("等待审批", "Waiting for Approval")
        case .waitingUser: localized("等待你的输入", "Waiting for Your Input")
        case .startingProcess: localized("正在启动服务", "Starting Service")
        case .readingLargeResult: localized("正在提取大结果", "Reading Large Result")
        case .restoringCapability: localized("正在读取或恢复 Capability", "Reading or Restoring Capability")
        case .retryScheduled: localized("等待重试", "Waiting to Retry")
        case .paused: localized("已暂停", "Paused")
        }
    }
}
