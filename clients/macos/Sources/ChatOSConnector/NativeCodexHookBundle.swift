import CryptoKit
import Foundation

struct NativePluginHookDefinition: Codable, Sendable, Equatable {
    var id: String
    var event: String
    var matcher: String?
    var command: String
    var isAsync: Bool
    var timeoutSeconds: Double
    var additionalContextLimit: Int

    private enum CodingKeys: String, CodingKey {
        case event, matcher, command
        case id = "hook_id"
        case isAsync = "is_async"
        case timeoutSeconds = "timeout_seconds"
        case additionalContextLimit = "additional_context_limit"
    }

    init(
        id: String,
        event: String,
        matcher: String?,
        command: String,
        isAsync: Bool,
        timeoutSeconds: Double,
        additionalContextLimit: Int
    ) {
        self.id = id
        self.event = event
        self.matcher = matcher
        self.command = command
        self.isAsync = isAsync
        self.timeoutSeconds = timeoutSeconds
        self.additionalContextLimit = additionalContextLimit
    }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decode(String.self, forKey: .id)
        event = try values.decode(String.self, forKey: .event)
        matcher = try values.decodeIfPresent(String.self, forKey: .matcher)
        command = try values.decode(String.self, forKey: .command)
        isAsync = try values.decodeIfPresent(Bool.self, forKey: .isAsync) ?? false
        timeoutSeconds = try values.decode(Double.self, forKey: .timeoutSeconds)
        additionalContextLimit = try values.decode(Int.self, forKey: .additionalContextLimit)
    }
}

struct NativePluginHookTrustRecord: Codable, Sendable, Equatable {
    static let runnerPolicyVersion: UInt32 = 1

    var bundleID: String
    var ownerUserID: String
    var pluginID: String
    var releaseID: String
    var componentKey: String
    var artifactSHA256: String
    var contentSHA256: String
    var normalizedHookSHA256: String
    var commandSHA256: String
    var permissionSnapshotSHA256: String
    var runnerPolicyVersion: UInt32
    var installationRoot: String
    var permissions: [String]
    var taskPolicy: NativePluginManifest.TaskPolicy?
    var hooks: [NativePluginHookDefinition]
    var trustedAtUnixMs: Int64

    private enum CodingKeys: String, CodingKey {
        case hooks, permissions
        case taskPolicy = "task_policy"
        case bundleID = "bundle_id"
        case ownerUserID = "owner_user_id"
        case pluginID = "plugin_id"
        case releaseID = "release_id"
        case componentKey = "component_key"
        case artifactSHA256 = "artifact_sha256"
        case contentSHA256 = "content_sha256"
        case normalizedHookSHA256 = "normalized_hook_sha256"
        case commandSHA256 = "command_sha256"
        case permissionSnapshotSHA256 = "permission_snapshot_sha256"
        case runnerPolicyVersion = "runner_policy_version"
        case installationRoot = "installation_root"
        case trustedAtUnixMs = "trusted_at_unix_ms"
    }
}

struct NativePluginHookBundle: Sendable, Equatable {
    var hooks: [NativePluginHookDefinition]
    var contentSHA256: String
    var normalizedSHA256: String

    var events: [String] { Array(Set(hooks.map(\.event))).sorted() }
    var asyncEvents: [String] { Array(Set(hooks.filter(\.isAsync).map(\.event))).sorted() }
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

enum NativePluginHookBundleParser {
    static func load(
        contribution: NativePluginManifest.HookContribution,
        installationRoot: URL
    ) throws -> NativePluginHookBundle {
        switch contribution.dialect.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case "chatos_v1":
            return try NativeChatOSHookBundleParser.load(
                contribution: contribution,
                installationRoot: installationRoot
            )
        case "codex":
            return try NativeCodexHookBundleParser.load(
                contribution: contribution,
                installationRoot: installationRoot
            )
        default:
            throw NativeConnectorError.pluginInstallation(
                "Hook \(contribution.componentKey) 使用未知 dialect：\(contribution.dialect)"
            )
        }
    }
}

private struct NativeChatOSHookSet: Decodable {
    struct Matcher: Decodable {
        var toolNames: [String] = []
        var toolKinds: [String] = []
        var agentKeys: [String] = []
        var componentKeys: [String] = []
        var outcomes: [String] = []

        var isEmpty: Bool {
            toolNames.isEmpty && toolKinds.isEmpty && agentKeys.isEmpty
                && componentKeys.isEmpty && outcomes.isEmpty
        }
    }

    struct Entrypoint: Decodable {
        var type: String
        var command: String
        var args: [String] = []
    }

    struct Hook: Decodable {
        var id: String
        var events: [String]
        var matcher: Matcher?
        var entrypoint: Entrypoint
        var timeoutMs: UInt64 = 5_000
        var maxOutputBytes: Int = 64 * 1_024
        var failurePolicy: String = "continue"
        var workspaceWrite: Bool = false
    }

    var schemaVersion: Int = 1
    var hooks: [Hook]
}

enum NativeChatOSHookBundleParser {
    private static let supportedEvents = Set([
        "SessionStart", "UserPromptSubmit", "SubagentStart",
        "PreToolUse", "PermissionRequest", "PostToolUse",
        "Stop", "SubagentStop", "Interrupt", "SessionEnd", "PostCompact",
    ])

    static func load(
        contribution: NativePluginManifest.HookContribution,
        installationRoot: URL
    ) throws -> NativePluginHookBundle {
        let sourceURL = try secureFileURL(
            contribution.path,
            installationRoot: installationRoot,
            field: "Hook 清单"
        )
        let data = try NativeBoundedFileReader.read(
            sourceURL,
            maximumBytes: NativeCodexHookBundleParser.maximumBundleBytes
        )
        return try parse(data, installationRoot: installationRoot)
    }

    static func parse(
        _ data: Data,
        installationRoot: URL
    ) throws -> NativePluginHookBundle {
        let hookSet: NativeChatOSHookSet
        do {
            hookSet = try JSONDecoder().decode(NativeChatOSHookSet.self, from: data)
        } catch {
            throw NativeConnectorError.pluginInstallation(
                "ChatOS Hook 清单不是有效的 chatos_v1 JSON：\(error.localizedDescription)"
            )
        }
        guard hookSet.schemaVersion == 1 else {
            throw NativeConnectorError.pluginInstallation(
                "ChatOS Hook schemaVersion 必须是 1"
            )
        }
        guard !hookSet.hooks.isEmpty,
              hookSet.hooks.count <= NativeCodexHookBundleParser.maximumHookCount else {
            throw NativeConnectorError.pluginInstallation("ChatOS Hook 数量必须在 1–64 之间")
        }

        var ids = Set<String>()
        var definitions: [NativePluginHookDefinition] = []
        var normalized: [[String: Any]] = []
        for hook in hookSet.hooks.sorted(by: { $0.id < $1.id }) {
            let id = hook.id.trimmingCharacters(in: .whitespacesAndNewlines)
            guard canonicalIdentifier(id), ids.insert(id).inserted else {
                throw NativeConnectorError.pluginInstallation("ChatOS Hook id 无效或重复：\(id)")
            }
            guard !hook.events.isEmpty else {
                throw NativeConnectorError.pluginInstallation("ChatOS Hook \(id) 缺少 events")
            }
            let events = Array(Set(hook.events)).sorted()
            guard events.count == hook.events.count,
                  events.allSatisfy(supportedEvents.contains) else {
                throw NativeConnectorError.pluginInstallation(
                    "ChatOS Hook \(id) 包含重复或当前 runtime 不支持的事件"
                )
            }
            guard hook.matcher?.isEmpty != false else {
                throw NativeConnectorError.pluginInstallation(
                    "当前 macOS runtime 尚未支持 chatos_v1 matcher，不能静默忽略"
                )
            }
            guard hook.entrypoint.type == "command" else {
                throw NativeConnectorError.pluginInstallation(
                    "ChatOS Hook \(id) 只支持 command entrypoint"
                )
            }
            guard (100...30_000).contains(hook.timeoutMs),
                  (1_024...256 * 1_024).contains(hook.maxOutputBytes),
                  hook.failurePolicy == "continue",
                  hook.workspaceWrite == false else {
                throw NativeConnectorError.pluginInstallation(
                    "ChatOS Hook \(id) 使用了当前 macOS runtime 无法无损执行的策略"
                )
            }
            guard hook.entrypoint.args.count <= 32,
                  hook.entrypoint.args.allSatisfy({
                      $0.utf8.count <= 4_096 && !$0.contains("\0")
                  }) else {
                throw NativeConnectorError.pluginInstallation(
                    "ChatOS Hook \(id) 的 entrypoint args 无效"
                )
            }

            let commandPath = normalizedCommandPath(hook.entrypoint.command)
            guard commandPath.hasPrefix("scripts/") || commandPath.hasPrefix("binaries/"),
                  commandPath.unicodeScalars.allSatisfy({ scalar in
                      CharacterSet.alphanumerics
                        .union(CharacterSet(charactersIn: "._-/"))
                        .contains(scalar)
                  }) else {
                throw NativeConnectorError.pluginInstallation(
                    "ChatOS Hook \(id) 的 command 必须位于 scripts/ 或 binaries/"
                )
            }
            let executable = try secureFileURL(
                commandPath,
                installationRoot: installationRoot,
                field: "Hook command"
            )
            guard FileManager.default.isExecutableFile(atPath: executable.path) else {
                throw NativeConnectorError.pluginInstallation(
                    "ChatOS Hook command 必须是可执行文件：\(commandPath)"
                )
            }
            let command = (["\"$PLUGIN_ROOT/\(commandPath)\""]
                + hook.entrypoint.args.map(shellQuote)).joined(separator: " ")
            for (eventIndex, event) in events.enumerated() {
                let wireID = events.count == 1 ? id : "\(id)-\(eventIndex)"
                guard canonicalIdentifier(wireID) else {
                    throw NativeConnectorError.pluginInstallation(
                        "ChatOS Hook \(id) 展开后的 id 超过限制"
                    )
                }
                definitions.append(.init(
                    id: wireID,
                    event: event,
                    matcher: nil,
                    command: command,
                    isAsync: false,
                    timeoutSeconds: Double(hook.timeoutMs) / 1_000,
                    additionalContextLimit: hook.maxOutputBytes
                ))
                normalized.append([
                    "id": wireID,
                    "event": event,
                    "command": command,
                    "async": false,
                    "timeoutMs": hook.timeoutMs,
                    "maxOutputBytes": hook.maxOutputBytes,
                    "failurePolicy": hook.failurePolicy,
                    "workspaceWrite": hook.workspaceWrite,
                ])
            }
        }
        guard definitions.count <= NativeCodexHookBundleParser.maximumHookCount else {
            throw NativeConnectorError.pluginInstallation("ChatOS Hook 展开后超过 64 个")
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

    private static func canonicalIdentifier(_ value: String) -> Bool {
        !value.isEmpty && value.utf8.count <= 256 && value.unicodeScalars.allSatisfy {
            CharacterSet.alphanumerics
                .union(CharacterSet(charactersIn: "_-"))
                .contains($0)
        }
    }

    private static func normalizedCommandPath(_ raw: String) -> String {
        var path = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        while path.hasPrefix("./") { path.removeFirst(2) }
        return path
    }

    private static func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\"'\"'") + "'"
    }

    private static func secureFileURL(
        _ rawPath: String,
        installationRoot: URL,
        field: String
    ) throws -> URL {
        let path = normalizedCommandPath(rawPath)
        guard !path.isEmpty,
              !path.hasPrefix("/"),
              !path.contains("\0"),
              !path.split(separator: "/").contains("..") else {
            throw NativeConnectorError.pluginInstallation("\(field) 必须位于插件目录内")
        }
        let root = installationRoot.standardizedFileURL.resolvingSymlinksInPath()
        let source = installationRoot.appendingPathComponent(path).standardizedFileURL
        let resolved = source.resolvingSymlinksInPath()
        let values = try source.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
        guard resolved.path.hasPrefix(root.path + "/"),
              values.isRegularFile == true,
              values.isSymbolicLink != true else {
            throw NativeConnectorError.pluginInstallation("\(field) 不存在或发生符号链接逃逸")
        }
        return source
    }

    private static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
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
    ) throws -> NativePluginHookBundle {
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

    static func parse(_ data: Data) throws -> NativePluginHookBundle {
        guard let root = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let eventMap = root["hooks"] as? [String: Any],
              !eventMap.isEmpty else {
            throw NativeConnectorError.pluginInstallation("Codex Hook 文件必须包含非空 hooks 对象")
        }
        var definitions: [NativePluginHookDefinition] = []
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
                    let isAsync = try boolean(handler["async"], field: "async")
                    if try boolean(handler["asyncRewake"], field: "asyncRewake") {
                        throw NativeConnectorError.pluginInstallation(
                            "当前版本尚未支持 asyncRewake Codex Hook，不能静默改为普通 async"
                        )
                    }
                    let id = "\(event)-\(groupIndex)-\(handlerIndex)"
                    definitions.append(.init(
                        id: id,
                        event: event,
                        matcher: matcher,
                        command: command,
                        isAsync: isAsync,
                        timeoutSeconds: timeout,
                        additionalContextLimit: contextLimit
                    ))
                    normalized.append([
                        "id": id,
                        "event": event,
                        "matcher": matcher ?? "",
                        "command": command,
                        "async": isAsync,
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

    private static func boolean(_ raw: Any?, field: String) throws -> Bool {
        guard let raw else { return false }
        guard let number = raw as? NSNumber,
              CFGetTypeID(number) == CFBooleanGetTypeID() else {
            throw NativeConnectorError.pluginInstallation("Codex Hook \(field) 必须是布尔值")
        }
        return number.boolValue
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
