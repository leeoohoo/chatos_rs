import CryptoKit
import Foundation

struct NativeCodexHookDefinition: Sendable, Equatable {
    var id: String
    var event: String
    var matcher: String?
    var command: String
    var timeoutSeconds: Double
    var additionalContextLimit: Int
}

struct NativeCodexHookBundle: Sendable, Equatable {
    var hooks: [NativeCodexHookDefinition]
    var contentSHA256: String
    var normalizedSHA256: String

    var events: [String] { Array(Set(hooks.map(\.event))).sorted() }
    var commands: [String] { hooks.map(\.command) }
    var requiredRuntimes: [String] {
        var result = Set<String>()
        for command in commands {
            if command.range(
                of: #"(^|[;&|()[:space:]])node([[:space:]]|$)"#,
                options: .regularExpression
            ) != nil {
                result.insert("node")
            }
        }
        return result.sorted()
    }
}

enum NativeCodexHookBundleParser {
    static let maximumBundleBytes = 512 * 1_024
    static let maximumHookCount = 64
    static let maximumCommandBytes = 8 * 1_024
    static let maximumMatcherBytes = 1_024
    static let maximumAdditionalContextBytes = 256 * 1_024
    static let defaultTimeoutSeconds = 5.0
    static let maximumTimeoutSeconds = 30.0

    private static let supportedEvents = Set([
        "SessionStart", "UserPromptSubmit", "SubagentStart",
        "PreToolUse", "PermissionRequest", "PostToolUse",
        "Stop", "SubagentStop", "Interrupt", "SessionEnd", "PostCompact",
    ])

    static func load(
        contribution: NativePluginManifest.HookContribution,
        installationRoot: URL
    ) throws -> NativeCodexHookBundle {
        guard contribution.dialect == "codex" else {
            throw NativeConnectorError.pluginInstallation(
                "Hook \(contribution.componentKey) 使用未知 dialect：\(contribution.dialect)"
            )
        }
        let sourceURL = try secureSourceURL(
            contribution.path,
            installationRoot: installationRoot
        )
        let data = try NativeBoundedFileReader.read(sourceURL, maximumBytes: maximumBundleBytes)
        return try parse(data)
    }

    static func parse(_ data: Data) throws -> NativeCodexHookBundle {
        guard let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let eventMap = root["hooks"] as? [String: Any],
              !eventMap.isEmpty else {
            throw NativeConnectorError.pluginInstallation("Codex Hook 文件必须包含非空 hooks 对象")
        }
        var definitions: [NativeCodexHookDefinition] = []
        var normalized: [[String: Any]] = []
        for event in eventMap.keys.sorted() {
            guard supportedEvents.contains(event) else {
                let detail = event == "PreCompact"
                    ? "当前 runtime 无法准确触发 PreCompact；请使用 PostCompact"
                    : "不支持的 Codex Hook 事件"
                throw NativeConnectorError.pluginInstallation("\(detail)：\(event)")
            }
            guard let groups = eventMap[event] as? [[String: Any]], !groups.isEmpty else {
                throw NativeConnectorError.pluginInstallation("Codex Hook \(event) 必须是非空数组")
            }
            for (groupIndex, group) in groups.enumerated() {
                let matcher = try normalizedMatcher(group["matcher"], event: event)
                guard let handlers = group["hooks"] as? [[String: Any]], !handlers.isEmpty else {
                    throw NativeConnectorError.pluginInstallation(
                        "Codex Hook \(event)[\(groupIndex)] 缺少 handlers"
                    )
                }
                for (handlerIndex, handler) in handlers.enumerated() {
                    guard definitions.count < maximumHookCount else {
                        throw NativeConnectorError.pluginInstallation("Codex Hook 数量超过 \(maximumHookCount) 个")
                    }
                    guard handler["type"] as? String == "command" else {
                        throw NativeConnectorError.pluginInstallation(
                            "阶段 A 只接受 command Hook；mcp_tool 将在工具 Hook 阶段接入"
                        )
                    }
                    if handler["async"] as? Bool == true {
                        throw NativeConnectorError.pluginInstallation(
                            "当前版本尚未支持 async Codex Hook，不能静默改为同步执行"
                        )
                    }
                    guard let rawCommand = handler["command"] as? String else {
                        throw NativeConnectorError.pluginInstallation("Codex Hook command 缺失")
                    }
                    let command = rawCommand.trimmingCharacters(in: .whitespacesAndNewlines)
                    guard !command.isEmpty,
                          command.utf8.count <= maximumCommandBytes,
                          !command.contains("\0") else {
                        throw NativeConnectorError.pluginInstallation("Codex Hook command 无效或超过限制")
                    }
                    let timeout = try timeoutSeconds(handler["timeout"])
                    let contextLimit = try additionalContextLimit(handler["additionalContextLimit"])
                    let id = "\(event)-\(groupIndex)-\(handlerIndex)"
                    definitions.append(.init(
                        id: id,
                        event: event,
                        matcher: matcher,
                        command: command,
                        timeoutSeconds: timeout,
                        additionalContextLimit: contextLimit
                    ))
                    normalized.append([
                        "id": id,
                        "event": event,
                        "matcher": matcher ?? "",
                        "command": command,
                        "timeoutSeconds": timeout,
                        "additionalContextLimit": contextLimit,
                    ])
                }
            }
        }
        let normalizedData = try JSONSerialization.data(
            withJSONObject: normalized,
            options: [.sortedKeys]
        )
        return .init(
            hooks: definitions,
            contentSHA256: sha256(data),
            normalizedSHA256: sha256(normalizedData)
        )
    }

    private static func normalizedMatcher(_ raw: Any?, event: String) throws -> String? {
        guard let raw else { return nil }
        guard let value = raw as? String else {
            throw NativeConnectorError.pluginInstallation("Codex Hook \(event) matcher 必须是字符串")
        }
        let matcher = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !matcher.isEmpty,
              matcher.utf8.count <= maximumMatcherBytes,
              !matcher.contains("\0") else {
            throw NativeConnectorError.pluginInstallation("Codex Hook \(event) matcher 无效")
        }
        do {
            _ = try NSRegularExpression(pattern: matcher)
        } catch {
            throw NativeConnectorError.pluginInstallation("Codex Hook \(event) matcher 不是有效正则表达式")
        }
        return matcher
    }

    private static func timeoutSeconds(_ raw: Any?) throws -> Double {
        guard let raw else { return defaultTimeoutSeconds }
        guard let number = raw as? NSNumber else {
            throw NativeConnectorError.pluginInstallation("Codex Hook timeout 必须是秒数")
        }
        let value = number.doubleValue
        guard value >= 0.1, value <= maximumTimeoutSeconds else {
            throw NativeConnectorError.pluginInstallation(
                "Codex Hook timeout 必须在 0.1–\(Int(maximumTimeoutSeconds)) 秒之间"
            )
        }
        return value
    }

    private static func additionalContextLimit(_ raw: Any?) throws -> Int {
        guard let raw else { return 64 * 1_024 }
        guard let number = raw as? NSNumber else {
            throw NativeConnectorError.pluginInstallation("additionalContextLimit 必须是整数")
        }
        let value = number.intValue
        guard number.doubleValue == Double(value),
              (0...maximumAdditionalContextBytes).contains(value) else {
            throw NativeConnectorError.pluginInstallation("additionalContextLimit 超过安全限制")
        }
        return value
    }

    private static func secureSourceURL(
        _ rawPath: String,
        installationRoot: URL
    ) throws -> URL {
        var path = rawPath.trimmingCharacters(in: .whitespacesAndNewlines)
        while path.hasPrefix("./") { path.removeFirst(2) }
        guard !path.isEmpty,
              !path.hasPrefix("/"),
              !path.contains("\0"),
              !path.split(separator: "/").contains("..") else {
            throw NativeConnectorError.pluginInstallation("Hook path 必须位于插件目录内")
        }
        let root = installationRoot.standardizedFileURL.resolvingSymlinksInPath()
        let source = installationRoot.appendingPathComponent(path).standardizedFileURL
        let resolved = source.resolvingSymlinksInPath()
        let values = try source.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
        guard resolved.path.hasPrefix(root.path + "/"),
              values.isRegularFile == true,
              values.isSymbolicLink != true else {
            throw NativeConnectorError.pluginInstallation("Hook path 不存在或发生符号链接逃逸")
        }
        return source
    }

    private static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}
