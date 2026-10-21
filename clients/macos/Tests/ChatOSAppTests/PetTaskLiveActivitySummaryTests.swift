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
}
