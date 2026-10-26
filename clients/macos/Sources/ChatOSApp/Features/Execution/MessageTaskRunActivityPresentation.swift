import ChatOSCore
import Foundation

enum MessageTaskRunActivityPresentation {
    static func title(
        forTaskStatus taskStatus: String?,
        fallback activityKind: MessageTaskRunActivityKind?,
        localized: (String, String) -> String
    ) -> String? {
        if let override = taskStatusTitleOverride(for: taskStatus, localized: localized) {
            return override
        }
        return activityKind.map { title(for: $0, localized: localized) }
    }

    static func taskStatusTitleOverride(
        for status: String?,
        localized: (String, String) -> String
    ) -> String? {
        switch normalized(status) {
        case "reviewing_followup": localized("审查追加要求", "Reviewing Follow-up")
        case "waiting_confirmation": localized("等待确认", "Awaiting Confirmation")
        default: nil
        }
    }

    static func taskStatusDetailOverride(
        for status: String?,
        localized: (String, String) -> String
    ) -> String? {
        switch normalized(status) {
        case "reviewing_followup": localized(
            "后续执行已暂停，正在审查追加要求",
            "Further execution is paused while the follow-up is reviewed"
        )
        case "waiting_confirmation": localized(
            "后续执行已暂停，等待你确认要求",
            "Further execution is paused awaiting your confirmation"
        )
        default: nil
        }
    }

    static func title(
        for kind: MessageTaskRunActivityKind,
        localized: (String, String) -> String
    ) -> String {
        switch kind {
        case .waitingDependency: localized("等待前置任务", "Waiting for Dependency")
        case .waitingModel: localized("等待 AI 响应", "Waiting for AI")
        case .runningModel: localized("AI 正在推理", "AI Is Reasoning")
        case .waitingTool: localized("正在调用工具", "Calling Tool")
        case .waitingApproval: localized("等待审批", "Waiting for Approval")
        case .waitingUser: localized("等待你的输入", "Waiting for Your Input")
        case .startingProcess: localized("正在启动服务", "Starting Service")
        case .readingLargeResult: localized("正在提取大结果", "Reading Large Result")
        case .restoringCapability: localized("正在加载工具能力", "Loading Tool Capability")
        case .retryScheduled: localized("等待重试", "Waiting to Retry")
        case .paused: localized("已暂停", "Paused")
        }
    }

    static func operationLabel(_ operation: String?) -> String? {
        guard let operation = operation?
            .trimmingCharacters(in: .whitespacesAndNewlines), !operation.isEmpty else { return nil }
        let knownNames = [
            "open_edit_session": "打开编辑会话",
            "stage_edit_batch": "暂存文件修改",
            "commit_edit_session": "提交文件修改",
            "abort_edit_session": "放弃文件修改",
            "read_file": "读取文件",
            "read_file_raw": "读取原始文件",
            "read_file_range": "读取文件片段",
            "search_files": "搜索文件",
            "search_text": "搜索代码文本",
            "list_dir": "列出目录",
            "execute_command": "执行命令",
            "process_wait": "等待命令完成",
            "process_poll": "检查命令状态",
            "process_log": "读取命令日志",
            "process_write": "向命令写入输入",
            "process_kill": "停止命令",
            "agent_skill_activate": "加载 Skill",
            "agent_skill_read_resource": "读取 Skill 资料",
            "capability_search": "查找工具能力",
            "capability_describe": "读取工具说明",
            "capability_invoke": "调用 Plugin 工具",
            "capture_browser_scenario": "浏览器自动化",
            "browser_session_open": "打开浏览器会话",
            "browser_cdp_attach": "连接浏览器页面",
            "browser_cdp_send": "操作浏览器页面",
            "browser_cdp_detach": "断开浏览器页面",
            "browser_navigate": "打开网页",
            "browser_click": "点击网页",
            "browser_type": "填写网页",
            "tool_result_query": "查询大结果",
            "tool_result_read": "读取结果片段",
            "task_run_process_allocate_service": "启动本地服务",
        ]
        let parts = operation.components(separatedBy: " · ")
        guard let rawTool = parts.last else { return operation }
        let readable = knownNames[rawTool]
            ?? rawTool.replacingOccurrences(of: "_", with: " ")
        return parts.count > 1
            ? "\(parts.dropLast().joined(separator: " · ")) · \(readable)"
            : readable
    }

    private static func normalized(_ status: String?) -> String {
        status?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() ?? ""
    }
}
