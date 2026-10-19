import ChatOSAgentRuntime
import ChatOSCore
import Foundation

actor NativeAgentCapabilityToolProvider: AgentToolProvider {
    static let searchToolName = NativeAgentCapabilityBrokerToolCatalog.searchToolName
    static let describeToolName = NativeAgentCapabilityBrokerToolCatalog.describeToolName
    static let activateSkillToolName = NativeAgentCapabilityBrokerToolCatalog.activateSkillToolName
    static let readSkillResourceToolName =
        NativeAgentCapabilityBrokerToolCatalog.readSkillResourceToolName
    static let invokeToolName = NativeAgentCapabilityBrokerToolCatalog.invokeToolName

    private enum CapabilityKind: Sendable {
        case builtIn
        case plugin(NativeInstalledAgentPlugin)
    }

    private struct CapabilityOption: Sendable {
        let token: String
        let name: String
        let description: String
        let kind: CapabilityKind
    }

    private struct SearchArguments: Decodable {
        let query: String?
    }

    private struct SelectionArguments: Decodable {
        let pluginOption: String
        let toolOption: String?
        let query: String?
        let offset: Int?
        let limit: Int?
        let schemaOffset: Int?
    }

    private struct InvokeArguments: Decodable {
        let pluginOption: String
        let toolOption: String
        let arguments: NativeJSONValue
    }

    private struct SkillActivationArguments: Decodable {
        let skillRef: String
    }

    private struct SkillResourceArguments: Decodable {
        let skillRef: String
        let relativePath: String
        let offset: Int?
        let maxChars: Int?
    }

    private let service: NativeLocalConnectorService
    private let ownerUserID: String
    private let runContext: LocalAgentChatRunContext
    private let projectContext: LocalConnectorPluginApplicationContext
    private let resolvedProject: NativeResolvedProjectPath?
    private let fallbackWorkspaceID: String?
    private let fallbackWorkspaceRootURL: URL?
    private let builtinCapabilities: Set<LocalAgentTodoBuiltinCapability>
    private let executionPresentation: NativeAgentPluginExecutionPresentation
    private let fallbackToSelectedOptions: Bool
    private let invokeBuiltinsThroughBroker: Bool
    private let productSkillBundle: LocalAgentSkillBundleSnapshot?
    private let selectedExternalMCPIDs: Set<String>
    private let restoredCapabilityDisclosures: Set<LocalRunCapabilityDisclosure>
    private let options: [CapabilityOption]
    private var registries: [String: AgentToolProviderRegistry] = [:]
    private var toolNamesByOption: [String: [String: String]] = [:]
    private var describedToolsByOption: [String: Set<String>] = [:]
    private var disclosedSkillNamesByOption: [String: Set<String>] = [:]
    private var activatedSkillNamesByOption: [String: Set<String>] = [:]
    private var pluginSkillSnapshotsByOption: [String: PluginSkillSnapshot] = [:]
    private var pluginSkillSessionsByOption: [String: PluginToolSkillSession] = [:]

    init(
        service: NativeLocalConnectorService,
        ownerUserID: String,
        runContext: LocalAgentChatRunContext,
        projectContext: LocalConnectorPluginApplicationContext,
        resolvedProject: NativeResolvedProjectPath?,
        fallbackWorkspaceID: String? = nil,
        fallbackWorkspaceRootURL: URL? = nil,
        builtinCapabilities: Set<LocalAgentTodoBuiltinCapability>,
        installedPlugins: [NativeInstalledAgentPlugin],
        executionPresentation: NativeAgentPluginExecutionPresentation = .agentGroupChat,
        fallbackToSelectedOptions: Bool = false,
        invokeBuiltinsThroughBroker: Bool = true,
        productSkillBundle: LocalAgentSkillBundleSnapshot? = nil,
        selectedExternalMCPIDs: Set<String> = [],
        preactivatedSkillRefs: Set<String> = [],
        restoredCapabilityDisclosures: Set<LocalRunCapabilityDisclosure> = []
    ) {
        self.service = service
        self.ownerUserID = ownerUserID
        self.runContext = runContext
        self.projectContext = projectContext
        self.resolvedProject = resolvedProject
        self.fallbackWorkspaceID = fallbackWorkspaceID
        self.fallbackWorkspaceRootURL = fallbackWorkspaceRootURL
        self.builtinCapabilities = builtinCapabilities
        self.executionPresentation = executionPresentation
        self.fallbackToSelectedOptions = fallbackToSelectedOptions
        self.invokeBuiltinsThroughBroker = invokeBuiltinsThroughBroker
        self.productSkillBundle = productSkillBundle
        self.selectedExternalMCPIDs = selectedExternalMCPIDs
        self.restoredCapabilityDisclosures = restoredCapabilityDisclosures
        let builtinOptions: [CapabilityOption] = builtinCapabilities.isEmpty && productSkillBundle == nil ? [] : [
            .init(
                token: "builtin_1",
                name: builtinCapabilities.isEmpty ? "ChatOS Skill Runtime" : "ChatOS 项目文件与终端",
                description: "当前执行已授权的基础能力：\(builtinCapabilities.map(\.rawValue).sorted().joined(separator: ", "))。Skill 加载不增加执行权限，真实工作目录由客户端绑定。",
                kind: .builtIn
            ),
        ]
        let options = builtinOptions + installedPlugins.enumerated().map { offset, plugin in
            .init(
                token: "plugin_\(offset + 1)",
                name: plugin.displayName,
                description: plugin.description,
                kind: .plugin(plugin)
            )
        }
        self.options = options
        var restored: [String: Set<String>] = [:]
        for skillRef in preactivatedSkillRefs {
            let productPrefix = "product-skill:"
            if skillRef.hasPrefix(productPrefix) {
                let skillName = String(skillRef.dropFirst(productPrefix.count))
                if !skillName.isEmpty,
                   let option = options.first(where: { option in
                       if case .builtIn = option.kind { return true }
                       return false
                   }) {
                    restored[option.token, default: []].insert(skillName)
                }
                continue
            }
            let prefix = "plugin-skill:"
            guard skillRef.hasPrefix(prefix) else { continue }
            let payload = String(skillRef.dropFirst(prefix.count))
            guard let separator = payload.firstIndex(of: ":") else { continue }
            let optionToken = String(payload[..<separator])
            let skillName = String(payload[payload.index(after: separator)...])
            guard !skillName.isEmpty,
                  options.contains(where: { $0.token == optionToken }) else { continue }
            restored[optionToken, default: []].insert(skillName)
        }
        activatedSkillNamesByOption = restored
    }

    func definitions() async throws -> [AgentToolDefinition] {
        NativeAgentCapabilityBrokerToolCatalog.definitions
    }

    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        do {
            return try await executeBrokerCall(call)
        } catch is CancellationError {
            throw CancellationError()
        } catch NativePluginRuntimeError.cancelled {
            throw CancellationError()
        } catch NativeAgentCapabilityDisclosure.Failure.invalidArguments {
            return .failure("能力描述参数无效。读取目录时 tool_option 和 schema_offset 必须省略或传 null，例如 {\"plugin_option\":\"plugin_1\",\"query\":\"browser_session_open\",\"tool_option\":null,\"schema_offset\":null}（plugin_option 使用搜索结果中的实际选项）。读取工具详情时使用目录返回的精确 tool_option，query 省略或传 null；schema_offset 仅用于工具明确返回的 schema 分页。不要填写占位字符。")
        } catch let error as NativePluginRuntimeError {
            return .failure(NativeAgentPluginFailurePresenter.message(
                for: error,
                projectRootURLs: [resolvedProject?.absoluteURL, fallbackWorkspaceRootURL]
                    .compactMap { $0 }
            ))
        } catch {
            return .failure("Plugin 能力启动或运行失败，请重新查看该能力后重试。")
        }
    }

    private func executeBrokerCall(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        switch call.name {
        case Self.searchToolName:
            let arguments = try decode(SearchArguments.self, from: call.arguments)
            let query = arguments.query?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            guard !query.isEmpty else {
                return .failure("请提供当前任务需要的能力关键词，不要枚举全部 Plugin。")
            }
            let terms = query.lowercased().split(whereSeparator: { $0.isWhitespace }).map(String.init)
            let lexicalMatches = options.filter { option in
                let builtInKeywords = if case .builtIn = option.kind {
                    " 文件 读写 代码 终端 shell command filesystem 需求调研 问卷 Human答案 备注 解决方案 执行计划 项目任务 survey requirement solution plan"
                } else {
                    ""
                }
                let searchable = "\(option.name) \(option.description)\(builtInKeywords)".lowercased()
                return terms.contains(where: searchable.contains)
            }
            // A local Task has already frozen and validated its Plugin selections. If a
            // translated or user-authored query has no literal overlap with an English
            // manifest, keep those selected capabilities discoverable instead of hiding
            // them behind a language-dependent lexical match.
            let visibleOptions = lexicalMatches.isEmpty && fallbackToSelectedOptions
                ? options
                : lexicalMatches
            let matches = visibleOptions.prefix(12).map { option in
                PluginSummary(
                    pluginOption: option.token,
                    name: option.name,
                    description: option.description
                )
            }
            return try outcome(SearchResponse(matches: Array(matches)))

        case Self.describeToolName:
            let arguments = try decode(SelectionArguments.self, from: call.arguments)
            guard let option = options.first(where: { $0.token == arguments.pluginOption }) else {
                return .failure("能力选项无效或已经过期，请重新搜索。")
            }
            let registry = try await registry(for: option)
            var names: [String: String] = [:]
            var requiredSkillNames = Set<String>()
            if case .builtIn = option.kind {
                requiredSkillNames.formUnion(availableRuntimeProductSkillNames)
            }
            let visibleDefinitions = visibleDefinitions(in: registry, for: option)
            let tools = try visibleDefinitions.enumerated().map { offset, definition in
                let token = "tool_\(offset + 1)"
                names[token] = definition.name
                let toolSkills = try catalogSkillNames(for: definition)
                requiredSkillNames.formUnion(toolSkills)
                let schemaText = redact(String(decoding: definition.schema, as: UTF8.self))
                let schema = try JSONDecoder().decode(NativeJSONValue.self, from: Data(schemaText.utf8))
                return ToolSummary(
                    toolOption: token,
                    name: definition.name,
                    description: redact(definition.description),
                    inputSchema: schema,
                    effect: definition.effect.rawValue,
                    requiredSkillRefs: toolSkills.map {
                        skillReference(named: $0, for: option)
                    },
                    invocationMode: invocationMode(for: option),
                    skillGate: try canonicalSkillGate(for: definition, option: option, skillNames: toolSkills)
                )
            }
            toolNamesByOption[option.token] = names
            var skills: [SkillSummary] = []
            for name in requiredSkillNames.sorted() {
                skills.append(try await skillSummary(named: name, for: option))
            }
            let descriptor = DescribeResponse(
                bindingSchemaVersion: 1,
                providerID: {
                    providerID(for: option)
                }(),
                pluginOption: option.token,
                name: option.name,
                tools: tools,
                skills: skills
            )
            let result = try NativeAgentCapabilityDisclosure.response(
                descriptor, toolOption: arguments.toolOption, query: arguments.query,
                offset: arguments.offset ?? 0, limit: arguments.limit ?? 8,
                schemaOffset: arguments.schemaOffset
            )
            recordDisclosedSkills(from: result, for: option)
            if capabilitySchemaIsComplete(in: result), let token = arguments.toolOption {
                describedToolsByOption[option.token, default: []].insert(token)
            }
            let rendered = try outcome(result)
            return .init(rendered.content, isError: result.jsonObject?["view"]?.jsonString == "error")

        case Self.activateSkillToolName:
            let arguments = try decode(SkillActivationArguments.self, from: call.arguments)
            let selection = try resolveSkillSelection(skillRef: arguments.skillRef)
            let option = selection.option
            let registry = try await registry(for: option)
            try restoreCapabilityDisclosures(for: option, registry: registry)
            var available: Set<String> = []
            if case .builtIn = option.kind {
                available.formUnion(availableRuntimeProductSkillNames)
            }
            for definition in registry.definitions {
                available.formUnion(try catalogSkillNames(for: definition))
            }
            guard available.contains(selection.skillName) else {
                return .failure("这个 Skill 不属于当前能力或当前可信执行计划。")
            }
            if case .plugin = option.kind,
               activatedSkillNamesByOption[option.token]?.contains(selection.skillName) != true,
               disclosedSkillNamesByOption[option.token]?.contains(selection.skillName) != true {
                return .failure(
                    "请先用 capability_describe 查看当前工具，并只激活返回的精确 skill_ref。"
                )
            }
            let document = try await activateSkill(named: selection.skillName, for: option)
            return try outcome(SkillActivationResponse(
                skillRef: selection.skillRef,
                instructions: document.instructions,
                resources: document.resourcePaths
            ))

        case Self.readSkillResourceToolName:
            let arguments = try decode(SkillResourceArguments.self, from: call.arguments)
            let selection = try resolveSkillSelection(skillRef: arguments.skillRef)
            let option = selection.option
            let page = try await readSkillResource(
                for: option,
                skillName: selection.skillName,
                relativePath: arguments.relativePath,
                offset: arguments.offset ?? 0,
                maximumCharacters: arguments.maxChars
                    ?? ProgressiveSkillFileLoader.maximumPageCharacters
            )
            return try outcome(SkillResourceResponse(
                skillRef: selection.skillRef,
                relativePath: arguments.relativePath,
                content: page.content,
                offset: page.offset,
                nextOffset: page.nextOffset,
                truncated: page.truncated
            ))

        case Self.invokeToolName:
            try Task.checkCancellation()
            let arguments = try decode(InvokeArguments.self, from: call.arguments)
            guard let option = options.first(where: { $0.token == arguments.pluginOption }) else {
                return .failure("请先搜索并查看该能力，再使用本轮返回的工具选项调用。")
            }
            let registry = try await registry(for: option)
            try restoreCapabilityDisclosures(for: option, registry: registry)
            guard let toolName = toolNamesByOption[option.token]?[arguments.toolOption] else {
                return .failure("请先搜索并查看该能力，再使用本轮返回的工具选项调用。")
            }
            guard arguments.arguments.jsonObject != nil else {
                return .failure("能力工具参数必须是 JSON 对象。")
            }
            if invocationMode(for: option) == "direct_task_tool" {
                return .failure(
                    "本地 Task 内置能力请通过原生工具 `\(toolName)` 直接调用，以应用 Task 审批、项目范围和进程归属校验；capability_invoke 仅执行 Plugin。"
                )
            }
            guard describedToolsByOption[option.token, default: []].contains(arguments.toolOption) else {
                return .failure("请先用 capability_describe 的 plugin_option + tool_option 读取所选工具的完整定义；目录索引不能代替工具 schema。")
            }
            guard let definition = registry.definitions.first(where: {
                $0.name == toolName
            }) else {
                return .failure("能力工具定义已经失效，请重新查看该能力。")
            }
            let argumentData = Data(arguments.arguments.canonicalJSONString.utf8)
            let missing = try await missingSkills(
                for: definition,
                arguments: argumentData,
                option: option
            )
            guard missing.isEmpty else {
                let missingRefs = missing.map {
                    skillReference(named: $0, for: option)
                }
                return .failure(
                    "调用此工具前必须先激活 Skill：\(missingRefs.joined(separator: ", "))。"
                )
            }
            let result = try await registry.execute(.init(
                id: call.id,
                name: toolName,
                arguments: arguments.arguments.canonicalJSONString
            ))
            try Task.checkCancellation()
            return .init(
                redact(result.content),
                madeProgress: result.madeProgress,
                isError: result.isError
            )

        default:
            return .failure("能力发现工具不可用：\(call.name)")
        }
    }

    private func visibleDefinitions(
        in registry: AgentToolProviderRegistry,
        for option: CapabilityOption
    ) -> [AgentToolDefinition] {
        registry.definitions.filter {
            skillBinding(for: $0)?.activationPolicy != .controlPlane && (
                invocationMode(for: option) != "direct_task_tool"
                    || NativeLocalAgentPlatformToolCatalog.taskExecutionToolNames.contains($0.name)
            )
        }
    }

    private func providerID(for option: CapabilityOption) -> String {
        switch option.kind {
        case .builtIn: ProductToolProviderID.capabilityBroker
        case .plugin(let plugin): "plugin:\(plugin.id)"
        }
    }

    private func capabilitySchemaIsComplete(in result: NativeJSONValue) -> Bool {
        guard result.jsonObject?["view"]?.jsonString == "tool",
              let tools = result.jsonObject?["tools"]?.jsonArray,
              tools.count == 1,
              let tool = tools.first?.jsonObject else {
            return false
        }
        if tool["input_schema"] != nil { return true }
        return tool["schema_delivery"]?.jsonString == "paged"
            && tool["schema_chunk"]?.jsonString != nil
            && tool["next_schema_offset"] == .null
    }

    private func restoreCapabilityDisclosures(
        for option: CapabilityOption,
        registry: AgentToolProviderRegistry
    ) throws {
        guard case .plugin = option.kind else { return }
        let providerID = providerID(for: option)
        let definitions = visibleDefinitions(in: registry, for: option)
        var names: [String: String] = [:]
        for (offset, definition) in definitions.enumerated() {
            let token = "tool_\(offset + 1)"
            names[token] = definition.name
            let schemaText = redact(String(decoding: definition.schema, as: UTF8.self))
            let schema = try JSONDecoder().decode(NativeJSONValue.self, from: Data(schemaText.utf8))
            let digest = NativePluginManifestLoader.sha256(schema.canonicalJSONString)
            guard restoredCapabilityDisclosures.contains(where: {
                $0.pluginOption == option.token
                    && $0.toolOption == token
                    && $0.providerID == providerID
                    && $0.schemaSHA256 == digest
            }) else {
                continue
            }
            describedToolsByOption[option.token, default: []].insert(token)
            disclosedSkillNamesByOption[option.token, default: []]
                .formUnion(try catalogSkillNames(for: definition))
        }
        toolNamesByOption[option.token] = names
    }

    private func registry(for option: CapabilityOption) async throws -> AgentToolProviderRegistry {
        if let existing = registries[option.token] { return existing }
        let providers: [any AgentToolProvider]
        switch option.kind {
        case .builtIn:
            if !invokeBuiltinsThroughBroker {
                providers = [NativeLocalTaskBuiltinDiscoveryProvider(
                    allowedCapabilities: builtinCapabilities
                )]
            } else {
                guard let resolvedProject else {
                    throw NativePluginRuntimeError.invalidRequest("当前会话没有绑定项目能力")
                }
                providers = [NativeAgentBuiltinToolProvider(
                    service: service,
                    runContext: runContext,
                    resolvedProject: resolvedProject,
                    allowedCapabilities: builtinCapabilities
                )]
            }
        case let .plugin(plugin):
            let pluginProviders = try await service.makeAgentPluginSkillToolProviders(
                ownerUserID: ownerUserID,
                runContext: runContext,
                plugins: [plugin],
                projectContext: projectContext,
                fallbackWorkspaceID: fallbackWorkspaceID,
                fallbackWorkspaceRootURL: fallbackWorkspaceRootURL,
                executionPresentation: executionPresentation
            )
            guard let first = pluginProviders.first else {
                throw NativePluginRuntimeError.invalidRequest("Plugin 没有可用的 MCP 组件")
            }
            pluginSkillSnapshotsByOption[option.token] = first.pluginSkillSnapshot
            pluginSkillSessionsByOption[option.token] = first.pluginSkillSession
            for skillName in activatedSkillNamesByOption[option.token, default: []].sorted() {
                _ = try await first.pluginSkillSession.activate(named: skillName)
            }
            providers = pluginProviders.map { $0 as any AgentToolProvider }
        }
        let registry = try await AgentToolProviderRegistry(providers: providers)
        if case .builtIn = option.kind {
            let available = Set(try registry.definitions.flatMap { definition in
                if !invokeBuiltinsThroughBroker,
                   !NativeLocalAgentPlatformToolCatalog.taskExecutionToolNames.contains(definition.name) {
                    return [String]()
                }
                return try catalogSkillNames(for: definition)
            }).union(availableRuntimeProductSkillNames)
            activatedSkillNamesByOption[option.token] =
                activatedSkillNamesByOption[option.token, default: []].intersection(available)
        }
        registries[option.token] = registry
        return registry
    }

    private func invocationMode(for option: CapabilityOption) -> String {
        if case .builtIn = option.kind, !invokeBuiltinsThroughBroker {
            return "direct_task_tool"
        }
        return "capability_invoke"
    }

    private func canonicalSkillGate(
        for definition: AgentToolDefinition,
        option: CapabilityOption,
        skillNames: [String]
    ) throws -> NativeJSONValue {
        guard let data = definition.pluginSkillGate else {
            return .object(["all_of": .array(skillNames.map {
                .string(skillReference(named: $0, for: option))
            })])
        }
        let gate = try PluginToolSkillGate.decode(data)
        var result: [String: NativeJSONValue] = [
            "all_of": .array(gate.allOf.map { .string(skillReference(named: $0, for: option)) }),
        ]
        if let selector = gate.selectByArgument {
            result["select_by_argument"] = .object([
                "pointer": .string(selector.pointer),
                "map": .object(selector.map.mapValues { .string(skillReference(named: $0, for: option)) }),
            ])
        }
        return .object(result)
    }

    private func catalogSkillNames(for definition: AgentToolDefinition) throws -> [String] {
        if let bundle = productSkillBundle,
           let providerID = definition.providerID,
           [ProductToolProviderID.projectRead, ProductToolProviderID.projectWrite, ProductToolProviderID.terminal].contains(providerID) {
            guard let binding = bundle.toolSkillBindings.first(where: {
                $0.toolNames.contains(definition.name)
            }), binding.activationPolicy != .controlPlane else { return [] }
            return Array(Set([binding.routerSkillRef, binding.specialistSkillRef]))
                .map { String($0.dropFirst("product-skill:".count)) }.sorted()
        }
        guard let binding = skillBinding(for: definition),
              binding.activationPolicy != .controlPlane else {
            guard let gateData = definition.pluginSkillGate else { return [] }
            return try PluginToolSkillGate.decode(gateData).catalogSkillNames
        }
        return binding.requiredSkillNames
    }

    private func skillSummary(
        named name: String,
        for option: CapabilityOption
    ) async throws -> SkillSummary {
        let skillRef = skillReference(named: name, for: option)
        switch option.kind {
        case .builtIn:
            if let document = try frozenProductDocument(named: name) {
                return .init(
                    skillRef: document.skillRef,
                    name: document.name,
                    role: document.role,
                    description: document.description
                )
            }
            let document = try BundledAgentSkillLoader.load(named: name)
            return .init(
                skillRef: skillRef,
                name: name,
                role: document.descriptor.role.rawValue,
                description: document.description
            )
        case .plugin:
            _ = try await registry(for: option)
            guard let snapshot = pluginSkillSnapshotsByOption[option.token] else {
                throw NativePluginRuntimeError.invalidRequest("Plugin Skill 固定快照不可用")
            }
            let document = try snapshot.document(named: name)
            return .init(
                skillRef: skillRef,
                name: name,
                role: document.role,
                description: document.description
            )
        }
    }

    private func skillReference(named name: String, for option: CapabilityOption) -> String {
        switch option.kind {
        case .builtIn:
            "product-skill:\(name)"
        case .plugin:
            "plugin-skill:\(option.token):\(name)"
        }
    }

    private func recordDisclosedSkills(
        from response: NativeJSONValue,
        for option: CapabilityOption
    ) {
        guard case .plugin = option.kind,
              let object = response.jsonObject,
              object["view"]?.jsonString != "error" else { return }
        let prefix = "plugin-skill:\(option.token):"
        let names = object["skills"]?.jsonArray?.compactMap { value -> String? in
            guard let skillRef = value.jsonObject?["skill_ref"]?.jsonString,
                  skillRef.hasPrefix(prefix) else { return nil }
            let name = String(skillRef.dropFirst(prefix.count))
            return name.isEmpty ? nil : name
        } ?? []
        disclosedSkillNamesByOption[option.token, default: []].formUnion(names)
    }

    private func resolveSkillSelection(
        skillRef rawSkillRef: String
    ) throws -> (option: CapabilityOption, skillName: String, skillRef: String) {
        let skillRef = rawSkillRef.trimmingCharacters(in: .whitespacesAndNewlines)
        let resolvedOption: CapabilityOption
        let resolvedName: String
        let productPrefix = "product-skill:"
        let pluginPrefix = "plugin-skill:"
        if skillRef.hasPrefix(productPrefix) {
            let name = String(skillRef.dropFirst(productPrefix.count))
            guard !name.isEmpty,
                  let option = options.first(where: { option in
                      if case .builtIn = option.kind { return true }
                      return false
                  }) else {
                throw NativePluginRuntimeError.invalidRequest("Skill 引用无效或已经过期")
            }
            resolvedName = name
            resolvedOption = option
        } else if skillRef.hasPrefix(pluginPrefix) {
            let payload = String(skillRef.dropFirst(pluginPrefix.count))
            guard let separator = payload.firstIndex(of: ":") else {
                throw NativePluginRuntimeError.invalidRequest("Skill 引用无效或已经过期")
            }
            let optionToken = String(payload[..<separator])
            let name = String(payload[payload.index(after: separator)...])
            guard !optionToken.isEmpty, !name.isEmpty,
                  let option = options.first(where: { option in
                      guard case .plugin = option.kind else { return false }
                      return option.token == optionToken
                  }) else {
                throw NativePluginRuntimeError.invalidRequest("Skill 引用无效或已经过期")
            }
            resolvedName = name
            resolvedOption = option
        } else {
            throw NativePluginRuntimeError.invalidRequest("Skill 引用无效或已经过期")
        }
        return (
            resolvedOption,
            resolvedName,
            skillReference(named: resolvedName, for: resolvedOption)
        )
    }

    private func activateSkill(
        named name: String,
        for option: CapabilityOption
    ) async throws -> (
        instructions: String,
        resourcePaths: [String]
    ) {
        switch option.kind {
        case .builtIn:
            if let document = try frozenProductDocument(named: name) {
                activatedSkillNamesByOption[option.token, default: []].insert(name)
                return (document.instructions, document.resources.map(\.relativePath))
            }
            let document = try BundledAgentSkillLoader.load(named: name)
            activatedSkillNamesByOption[option.token, default: []].insert(name)
            return (document.instructions, document.resourcePaths)
        case .plugin:
            _ = try await registry(for: option)
            guard let session = pluginSkillSessionsByOption[option.token] else {
                throw NativePluginRuntimeError.invalidRequest("Plugin Skill 固定快照不可用")
            }
            let document = try await session.activate(named: name)
            activatedSkillNamesByOption[option.token, default: []].insert(name)
            return (document.instructions, document.resourcePaths)
        }
    }

    private func readSkillResource(
        for option: CapabilityOption,
        skillName: String,
        relativePath: String,
        offset: Int,
        maximumCharacters: Int
    ) async throws -> ProgressiveSkillFileLoader.TextPage {
        switch option.kind {
        case .builtIn:
            _ = try await registry(for: option)
            guard activatedSkillNamesByOption[option.token]?.contains(skillName) == true else {
                throw NativePluginRuntimeError.invalidRequest(
                    "请先激活当前能力列出的 Skill，再读取它的参考资料。"
                )
            }
            if let document = try frozenProductDocument(named: skillName) {
                guard let resource = document.resources.first(where: {
                    $0.relativePath == relativePath
                }) else {
                    throw NativePluginRuntimeError.invalidRequest("Skill 固定快照中不存在该资源")
                }
                let scalars = Array(resource.content.unicodeScalars)
                guard offset >= 0, offset <= scalars.count, maximumCharacters > 0 else {
                    throw ProgressiveSkillFileLoader.LoaderError.invalidOffset
                }
                let end = offset + min(maximumCharacters, scalars.count - offset)
                return .init(
                    content: String(String.UnicodeScalarView(scalars[offset..<end])),
                    offset: offset,
                    nextOffset: end < scalars.count ? end : nil,
                    truncated: end < scalars.count
                )
            }
            return try BundledAgentSkillLoader.readResource(
                skillName: skillName,
                relativePath: relativePath,
                offset: offset,
                maximumCharacters: maximumCharacters
            )
        case .plugin:
            _ = try await registry(for: option)
            guard let session = pluginSkillSessionsByOption[option.token] else {
                throw NativePluginRuntimeError.invalidRequest("Plugin Skill 固定快照不可用")
            }
            return try await session.readResource(
                skillName: skillName,
                relativePath: relativePath,
                offset: offset,
                maximumCharacters: maximumCharacters
            )
        }
    }

    private func missingSkills(
        for definition: AgentToolDefinition,
        arguments: Data,
        option: CapabilityOption
    ) async throws -> [String] {
        if let gateData = definition.pluginSkillGate {
            guard let session = pluginSkillSessionsByOption[option.token] else {
                throw NativePluginRuntimeError.invalidRequest("Plugin Skill 固定快照不可用")
            }
            return try await session.missingSkills(
                for: PluginToolSkillGate.decode(gateData),
                arguments: arguments
            )
        }
        let required = Set(try catalogSkillNames(for: definition))
        let activated = activatedSkillNamesByOption[option.token] ?? []
        return required.subtracting(activated).sorted()
    }

    private func frozenProductDocument(named name: String) throws -> LocalAgentSkillDocumentSnapshot? {
        guard let bundle = productSkillBundle else { return nil }
        guard let document = bundle.documents.first(where: {
            $0.skillRef == "product-skill:\(name)" && $0.name == name
        }) else {
            throw NativePluginRuntimeError.invalidRequest("Skill 不属于当前 Run 的 Product 固定快照")
        }
        return document
    }

    private var availableRuntimeProductSkillNames: Set<String> {
        Set(productSkillBundle?.toolSkillBindings
            .filter { binding in
                binding.activationPolicy == .runBound || selectedExternalMCPIDs.contains(where: {
                    binding.providerID == "chatos.external-mcp:\($0)"
                })
            }
            .flatMap { [$0.routerSkillRef, $0.specialistSkillRef] }
            .map { String($0.dropFirst("product-skill:".count)) } ?? [])
    }

    private func skillBinding(for definition: AgentToolDefinition) -> ToolSkillBinding? {
        guard let providerID = definition.providerID,
              let bindingID = definition.skillBindingID else { return nil }
        return ToolSkillCoverageCatalog.product.binding(
            providerID: providerID,
            skillBindingID: bindingID
        )
    }

    private func decode<Value: Decodable>(_ type: Value.Type, from json: String) throws -> Value {
        do {
            let decoder = JSONDecoder()
            decoder.keyDecodingStrategy = .convertFromSnakeCase
            return try decoder.decode(Value.self, from: Data(json.utf8))
        } catch {
            throw NativePluginRuntimeError.invalidRequest("能力发现工具参数无效")
        }
    }

    private func outcome<Value: Encodable>(_ value: Value) throws -> AgentToolOutcome {
        let encoder = JSONEncoder()
        encoder.keyEncodingStrategy = .convertToSnakeCase
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return .init(redact(String(decoding: try encoder.encode(value), as: UTF8.self)))
    }

    private func redact(_ value: String) -> String {
        value.replacingOccurrences(of: runContext.projectID, with: "[internal-project]")
    }
}
