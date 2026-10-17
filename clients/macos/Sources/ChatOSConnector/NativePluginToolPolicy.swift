import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    static let approvalEvaluationMaximumCharacters = 160_000

    static func safeArgumentSummary(
        toolName: String,
        arguments: NativeJSONValue
    ) -> String {
        if toolName == "browser_session_open" {
            let object = arguments.jsonObject ?? [:]
            let sessionName = object["session_name"]?.jsonString ?? "ChatOS Browser"
            if object["mode"]?.jsonString == "chrome_extension" {
                return "连接用户已主动配对的 Google Chrome：任务 \(sessionName)，仅控制本任务新建页面和用户主动共享给 ChatOS 的页面；不会枚举或控制其他现有标签页。任务新建页面进入同名原生标签组。"
            }
            return "当前 Chrome 尚未授权，自动使用 ChatOS 隔离浏览器：任务 \(sessionName)。"
        }
        if toolName == "browser_cdp_attach" {
            return "为当前隔离浏览器中的指定标签页建立临时 CDP 会话；浏览器会话 ID 与标签页 ID 均为 ChatOS 生成的不透明标识。"
        }
        if toolName == "browser_cdp_detach" {
            return "结束当前隔离浏览器中的临时 CDP 会话。"
        }
        if toolName == "browser_cdp_send" {
            let object = arguments.jsonObject ?? [:]
            let method = object["method"]?.jsonString ?? "未知方法"
            let target = object["target"]?.jsonString ?? "page"
            let parameterKeys = object["params"]?.jsonObject?.keys.sorted().joined(separator: ", ")
                ?? "无"
            let params = object["params"]?.jsonObject ?? [:]
            let expression = params["expression"]?.jsonString
            let expressionSummary = safeCDPExpressionSummary(expression)
            let mouseSummary = safeCDPMouseEventSummary(method: method, params: params)
            return "向当前隔离浏览器发送 CDP 方法 \(method)，目标 \(target)，参数字段：\(parameterKeys)\(expressionSummary.map { "；表达式：\($0)" } ?? "")\(mouseSummary ?? "")。"
        }
        let keys = arguments.jsonObject?.keys.sorted().joined(separator: ", ") ?? "参数"
        let digest = (try? NativePluginHash.canonicalSHA256(arguments).prefix(12)) ?? "unknown"
        return "字段：\(keys)；内容摘要：\(digest)"
    }

    static func safeApprovalEvaluationSummary(
        toolName: String,
        arguments: NativeJSONValue
    ) -> String {
        guard toolName == "browser_cdp_send" else {
            return safeArgumentSummary(toolName: toolName, arguments: arguments)
        }
        let object = arguments.jsonObject ?? [:]
        let method = object["method"]?.jsonString ?? "未知方法"
        let target = object["target"]?.jsonString ?? "page"
        let parameterKeys = object["params"]?.jsonObject?.keys.sorted().joined(separator: ", ")
            ?? "无"
        let params = object["params"]?.jsonObject ?? [:]
        let expression = params["expression"]?.jsonString
        let expressionSummary = safeCDPExpressionEvaluationSummary(expression)
        let mouseSummary = safeCDPMouseEventSummary(method: method, params: params)
        return "向当前隔离浏览器发送 CDP 方法 \(method)，目标 \(target)，参数字段：\(parameterKeys)\(expressionSummary.map { "；完整表达式：\($0)" } ?? "")\(mouseSummary ?? "")。"
    }

    static func browserSessionArguments(
        arguments: NativeJSONValue,
        contextBody: [String: NativeJSONValue],
        browserExtensionPaired: Bool = true
    ) -> NativeJSONValue {
        let requestedObject = arguments.jsonObject ?? [:]
        var object: [String: NativeJSONValue] = [:]
        if let sessionName = requestedObject["session_name"]?.jsonString?.nonEmptyTrimmed {
            object["session_name"] = .string(String(sessionName.prefix(80)))
        }
        object["mode"] = .string(browserExtensionPaired ? "chrome_extension" : "managed")
        if object["session_name"]?.jsonString?.nonEmptyTrimmed == nil {
            let title = contextBody["task_title"]?.jsonString?.nonEmptyTrimmed
                ?? contextBody["task_id"]?.jsonString?.nonEmptyTrimmed.map {
                    "ChatOS · \(String($0.prefix(12)))"
                }
                ?? contextBody["task_run_id"]?.jsonString?.nonEmptyTrimmed.map {
                    "ChatOS · \(String($0.prefix(12)))"
                }
                ?? "ChatOS Browser"
            object["session_name"] = .string(String(title.prefix(80)))
        }
        return .object(object)
    }

    static func permissionDescription(
        toolName: String,
        requiredPermissions: Set<String>
    ) -> String {
        let permissions = requiredPermissions.sorted().joined(separator: ", ")
        if toolName == "browser_session_open" {
            if requiredPermissions.contains("browser.managed.launch") {
                return "Chrome 扩展尚未授权，自动启动 ChatOS 隔离浏览器。所需权限：\(permissions)"
            }
            return "用户已主动配对 Chrome 扩展，当前任务也已选择 Browser 能力。Browser Bridge 只向任务暴露本任务新建页面和用户主动共享给 ChatOS 的页面，不会枚举或控制其他现有标签页；任务新建页面会进入同名原生标签组。所需权限：\(permissions)"
        }
        if toolName.hasPrefix("browser_cdp_") {
            return "仅操作当前 ChatOS 隔离浏览器会话中的临时 CDP 连接。所需权限：\(permissions)"
        }
        return permissions.isEmpty ? "未声明额外权限。" : "所需权限：\(permissions)"
    }

    static func deterministicAutoApprovalReason(
        componentKey: String,
        toolName: String,
        arguments: NativeJSONValue,
        requiredPermissions: Set<String>
    ) -> String? {
        guard componentKey == NativeBrowserPluginIdentity.componentKey,
              toolName == "browser_session_open",
              arguments.jsonObject?["mode"]?.jsonString == "chrome_extension",
              requiredPermissions == Set(["browser.chrome.attach"]) else {
            return nil
        }
        return "已配对 Chrome，且 Browser Bridge 仅暴露本任务新建页和用户主动共享页；由宿主在既定权限边界内批准。"
    }

    static func toolPolicy(
        _ tool: NativeJSONValue?,
        componentKey: String,
        toolName: String
    ) -> NativePluginToolPolicy {
        let meta = tool?.jsonObject?["_meta"]?.jsonObject
        let approval = meta?["chatos/approvalMode"]?.jsonString ?? "none"
        let risk = meta?["chatos/riskLevel"]?.jsonString ?? "low"
        let declaredTimeout = Int(
            meta?["chatos/timeoutMs"]?.jsonNumber
                ?? defaultPluginToolTimeoutMilliseconds(
                    componentKey: componentKey,
                    toolName: toolName
                )
        )
        let required = Set(
            meta?["chatos/requiredPermissions"]?.jsonArray?.compactMap(\.jsonString) ?? []
        )
        let rules: [NativePluginPermissionRule] = meta?["chatos/permissionRules"]?
            .jsonArray?.compactMap { value in
                guard let object = value.jsonObject,
                      let pointer = object["argumentPointer"]?.jsonString,
                      let permissions = object["requiredPermissions"]?.jsonArray else {
                    return nil
                }
                return NativePluginPermissionRule(
                    argumentPointer: pointer,
                    expectedValue: object["equals"] ?? .null,
                    matchWhenMissing: object["matchWhenMissing"]?.jsonBool ?? false,
                    requiredPermissions: Set(permissions.compactMap(\.jsonString))
                )
            } ?? []
        return .init(
            approvalMode: approval == "per_call" ? "per_call" : "none",
            riskLevel: ["low", "medium", "high", "critical"].contains(risk) ? risk : "low",
            timeoutMilliseconds: pluginToolHostTimeoutMilliseconds(
                declaredTimeoutMilliseconds: declaredTimeout
            ),
            requiredPermissions: required,
            permissionRules: rules
        )
    }

    static func defaultPluginToolTimeoutMilliseconds(
        componentKey: String,
        toolName: String
    ) -> Double {
        _ = componentKey
        _ = toolName
        return 7_200_000
    }

    static func pluginToolHostTimeoutMilliseconds(
        declaredTimeoutMilliseconds: Int
    ) -> Int {
        let bounded = min(7_200_000, max(300, declaredTimeoutMilliseconds))
        guard bounded < 7_200_000 else { return bounded }
        let grace = min(10_000, max(2_000, bounded / 2))
        return min(7_200_000, bounded + grace)
    }

    private static func safeCDPExpressionSummary(_ expression: String?) -> String? {
        guard let expression = expression?.trimmingCharacters(in: .whitespacesAndNewlines),
              !expression.isEmpty else { return nil }
        let sensitiveMarkers = [
            "authorization", "bearer", "password", "passwd", "secret", "token",
            "document.cookie", "localstorage", "sessionstorage",
        ]
        let normalized = expression.lowercased()
        guard !sensitiveMarkers.contains(where: normalized.contains) else {
            return "已隐藏敏感表达式"
        }
        return String(expression.replacingOccurrences(of: "\n", with: " ").prefix(240))
    }

    private static func safeCDPExpressionEvaluationSummary(_ expression: String?) -> String? {
        guard let expression = expression?.trimmingCharacters(in: .whitespacesAndNewlines),
              !expression.isEmpty else { return nil }
        let sensitiveMarkers = [
            "authorization", "bearer", "password", "passwd", "secret", "token",
            "document.cookie", "localstorage", "sessionstorage",
        ]
        let normalized = expression.lowercased()
        guard !sensitiveMarkers.contains(where: normalized.contains) else {
            return "已隐藏敏感表达式"
        }
        let flattened = expression.replacingOccurrences(of: "\n", with: " ")
        guard flattened.count > approvalEvaluationMaximumCharacters else { return flattened }
        return String(flattened.prefix(approvalEvaluationMaximumCharacters))
            + "…（仅审批评估摘要超过 160,000 字符，已安全截断）"
    }

    private static func safeCDPMouseEventSummary(
        method: String,
        params: [String: NativeJSONValue]
    ) -> String? {
        guard method == "Input.dispatchMouseEvent",
              let eventType = params["type"]?.jsonString else { return nil }
        let coordinates: String = {
            guard let x = params["x"]?.jsonNumber,
                  let y = params["y"]?.jsonNumber else { return "未提供坐标" }
            return "页面坐标（\(compactNumber(x)), \(compactNumber(y))）"
        }()
        switch eventType {
        case "mouseMoved":
            return "；实际动作：仅将鼠标移动到\(coordinates)，不会按下、释放或点击"
        case "mousePressed":
            let button = params["button"]?.jsonString ?? "未指定按键"
            let clickCount = params["clickCount"]?.jsonNumber.map(compactNumber) ?? "未指定"
            return "；实际动作：在\(coordinates)按下鼠标（\(button)，点击计数 \(clickCount)），尚未释放"
        case "mouseReleased":
            let button = params["button"]?.jsonString ?? "未指定按键"
            return "；实际动作：在\(coordinates)释放鼠标（\(button)），可能完成一次点击"
        case "mouseWheel":
            let deltaX = params["deltaX"]?.jsonNumber.map(compactNumber) ?? "0"
            let deltaY = params["deltaY"]?.jsonNumber.map(compactNumber) ?? "0"
            return "；实际动作：在\(coordinates)滚动页面（横向 \(deltaX)，纵向 \(deltaY)），不会点击"
        default:
            return "；实际动作：鼠标事件 \(eventType)，位置为\(coordinates)"
        }
    }

    private static func compactNumber(_ value: Double) -> String {
        value.rounded() == value ? String(Int64(value)) : String(value)
    }
}

struct NativePluginToolPolicy {
    var approvalMode: String
    var riskLevel: String
    var timeoutMilliseconds: Int
    var requiredPermissions: Set<String>
    var permissionRules: [NativePluginPermissionRule]

    func requiredPermissions(for arguments: NativeJSONValue) -> Set<String> {
        permissionRules.reduce(into: requiredPermissions) { result, rule in
            let value = arguments.value(atJSONPointer: rule.argumentPointer)
            if value == rule.expectedValue || (value == nil && rule.matchWhenMissing) {
                result.formUnion(rule.requiredPermissions)
            }
        }
    }
}

struct NativePluginPermissionRule {
    var argumentPointer: String
    var expectedValue: NativeJSONValue
    var matchWhenMissing: Bool
    var requiredPermissions: Set<String>
}

private extension String {
    var nonEmptyTrimmed: String? {
        let result = trimmingCharacters(in: .whitespacesAndNewlines)
        return result.isEmpty ? nil : result
    }
}
