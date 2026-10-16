import ChatOSAgentRuntime
import ChatOSCore
import Foundation

enum NativeAgentCapabilityBrokerToolCatalog {
    static let searchToolName = "capability_search"
    static let describeToolName = "capability_describe"
    static let activateSkillToolName = "agent_skill_activate"
    static let readSkillResourceToolName = "agent_skill_read_resource"
    static let invokeToolName = "capability_invoke"

    static let readOnlyToolNames: Set<String> = [
        searchToolName,
        describeToolName,
        activateSkillToolName,
        readSkillResourceToolName,
    ]
    static let toolNames = readOnlyToolNames.union([invokeToolName])

    static let definitions: [AgentToolDefinition] = [
        .init(
            name: searchToolName,
            description: "按任务关键词搜索本机已安装能力。只返回匹配 Plugin 的本轮临时选项和简介，不启动 Plugin，也不展开全部工具。",
            schema: schema(
                #"{"type":"object","properties":{"query":{"type":"string","minLength":1,"maxLength":200}},"required":["query"],"additionalProperties":false}"#
            ),
            providerID: ProductToolProviderID.capabilityBroker,
            skillBindingID: ProductToolSkillBindingID.capabilityBroker
        ),
        .init(
            name: describeToolName,
            description: "按 plugin_option 查看分页的工具/Skill 简短索引，不展开全部 schema。目录请求 tool_option 和 schema_offset 必须省略或传 null，不能填占位值；用 query 筛选，offset=next_offset 翻页。选中后用精确 tool_option 读取该工具完整 schema，此时 query 省略或传 null；只有工具明确返回 schema 分页时才传 schema_offset。",
            schema: schema(
                #"{"type":"object","properties":{"plugin_option":{"type":"string","minLength":1,"maxLength":80},"tool_option":{"type":["string","null"],"minLength":1,"maxLength":80,"description":"Exact option returned by the index for a single tool definition. Omit or use null for an index; never invent a placeholder."},"query":{"type":["string","null"],"minLength":1,"maxLength":200,"description":"Filter index by tool names or keywords. Omit or use null with tool_option."},"offset":{"type":["integer","null"],"minimum":0,"default":0,"description":"Index offset; use returned next_offset. Omit or null means 0."},"limit":{"type":["integer","null"],"minimum":1,"maximum":20,"default":8,"description":"Index page size. Omit or null means 8."},"schema_offset":{"type":["integer","null"],"minimum":0,"description":"Omit or use null unless this exact tool explicitly returned a paged schema; then use next_schema_offset. Never use 0 as an unset value."}},"required":["plugin_option"],"additionalProperties":false}"#
            ),
            providerID: ProductToolProviderID.capabilityBroker,
            skillBindingID: ProductToolSkillBindingID.capabilityBroker
        ),
        .init(
            name: activateSkillToolName,
            description: "激活 capability_describe 返回的一个精确 skill_ref，并加载核心指令。只可激活当前能力真实工具引用的 Skill。",
            schema: schema(
                #"{"type":"object","properties":{"skill_ref":{"type":"string","minLength":1,"maxLength":512}},"required":["skill_ref"],"additionalProperties":false}"#
            ),
            providerID: ProductToolProviderID.capabilityBroker,
            skillBindingID: ProductToolSkillBindingID.capabilityBroker
        ),
        .init(
            name: readSkillResourceToolName,
            description: "分页读取一个已激活 Plugin Skill 的参考资料。只读取当前任务需要的资料。",
            schema: schema(
                #"{"type":"object","properties":{"skill_ref":{"type":"string","minLength":1,"maxLength":512},"relative_path":{"type":"string","minLength":1,"maxLength":1024},"offset":{"type":"integer","minimum":0,"default":0},"max_chars":{"type":"integer","minimum":1,"maximum":160000,"default":160000}},"required":["skill_ref","relative_path"],"additionalProperties":false}"#
            ),
            providerID: ProductToolProviderID.capabilityBroker,
            skillBindingID: ProductToolSkillBindingID.capabilityBroker
        ),
        .init(
            name: invokeToolName,
            description: "调用已经通过 capability_describe 展开的一个本机 Plugin 工具。若工具声明 required_skill_refs，必须先逐个激活；plugin_option 和 tool_option 使用本轮临时选项。",
            schema: schema(
                #"{"type":"object","properties":{"plugin_option":{"type":"string","minLength":1,"maxLength":80},"tool_option":{"type":"string","minLength":1,"maxLength":80},"arguments":{"type":"object"}},"required":["plugin_option","tool_option","arguments"],"additionalProperties":false}"#
            ),
            effect: .write,
            providerID: ProductToolProviderID.capabilityBroker,
            skillBindingID: ProductToolSkillBindingID.capabilityBroker
        ),
    ]

    static let localAgentCapabilityTools: [LocalAgentJSONValue] = definitions.map { definition in
        guard let decoded = try? JSONDecoder().decode(
            LocalAgentJSONValue.self,
            from: definition.schema
        ) else {
            preconditionFailure("Capability broker schema is invalid: \(definition.name)")
        }
        var tool: [String: LocalAgentJSONValue] = [
            "type": .string("function"),
            "name": .string(definition.name),
            "description": .string(definition.description),
            "parameters": decoded,
        ]
        if definition.name == describeToolName { tool["strict"] = .bool(false) }
        return .object(tool)
    }

    /// Refresh existing advertised broker tools only, without granting new capabilities.
    static func upgradingDiscoverySchemas(_ tools: [LocalAgentJSONValue]) -> [LocalAgentJSONValue] {
        var seen = Set<String>()
        return tools.compactMap { tool in
            guard case let .object(object) = tool, case let .string(name)? = object["name"] else {
                return tool
            }
            guard let current = localAgentCapabilityTools.first(where: {
                guard case let .object(candidate) = $0 else { return false }
                return candidate["name"] == .string(name)
            }) else {
                return name.hasPrefix("capability_") || name.hasPrefix("agent_skill_")
                    ? nil : tool
            }
            return seen.insert(name).inserted ? current : nil
        }
    }


    private static func schema(_ value: String) -> Data { Data(value.utf8) }
}
