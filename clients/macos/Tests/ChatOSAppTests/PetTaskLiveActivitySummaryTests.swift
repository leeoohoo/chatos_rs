@testable import ChatOSApp
import ChatOSCore
import Foundation
import Testing

struct PetTaskLiveActivitySummaryTests {
    @Test("compact activity makes plugin tool names readable")
    func operationLabels() {
        #expect(PetTaskLiveActivitySummary.operationLabel(
            "Browser · capture_browser_scenario"
        ) == "Browser · 浏览器自动化")
        #expect(PetTaskLiveActivitySummary.operationLabel(
            "custom_tool_name"
        ) == "custom tool name")
    }

    @Test("compact activity reports the newest durable process milestone")
    func recentProgress() {
        let task = MessageTask(
            id: "task-1",
            title: "Deploy",
            status: "running",
            processLog: """
            [2026-10-21T08:00:00Z] 准备部署
            已完成依赖检查

            [2026-10-21T08:00:12Z] 部署前端
            静态资源已上传
            """
        )

        let progress = PetTaskLiveActivitySummary.recentProgress(in: task)
        #expect(progress?.title == "部署前端")
        #expect(progress?.occurredAtDate != nil)
    }

    @Test("task follow-up gates override a stale waiting-model activity")
    func followupGateOverridesRunActivity() {
        let localized: (String, String) -> String = { chinese, _ in chinese }

        #expect(MessageTaskRunActivityPresentation.title(
            forTaskStatus: " reviewing_followup ",
            fallback: .waitingModel,
            localized: localized
        ) == "审查追加要求")
        #expect(MessageTaskRunActivityPresentation.taskStatusDetailOverride(
            for: "reviewing_followup",
            localized: localized
        ) == "后续执行已暂停，正在审查追加要求")
        #expect(MessageTaskRunActivityPresentation.title(
            forTaskStatus: "waiting_confirmation",
            fallback: .runningModel,
            localized: localized
        ) == "等待确认")
        #expect(MessageTaskRunActivityPresentation.title(
            forTaskStatus: "running",
            fallback: .waitingModel,
            localized: localized
        ) == "等待 AI 响应")
    }
}
