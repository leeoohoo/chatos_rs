import ChatOSAgentRuntime
import ChatOSCore
import Foundation

enum NativeAgentCapabilityBrokerToolCatalog {
    static let searchToolName = "capability_search"
    static let describeToolName = "capability_describe"
    static let activateSkillToolName = "capability_skill_activate"
    static let readSkillResourceToolName = "capability_skill_read_resource"
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
            description: "按 capability_search 返回的临时 plugin_option，惰性启动一个能力，并读取本轮工具 schema、所需 Skill Router 和叶子目录。调用带 required_skills 的工具前必须逐个激活。",
            schema: schema(
                #"{"type":"object","properties":{"plugin_option":{"type":"string","minLength":1,"maxLength":80}},"required":["plugin_option"],"additionalProperties":false}"#
            ),
            providerID: ProductToolProviderID.capabilityBroker,
            skillBindingID: ProductToolSkillBindingID.capabilityBroker
        ),
        .init(
            name: activateSkillToolName,
            description: "激活 capability_describe 为该能力列出的一个产品 Skill 或固定 Plugin Skill，返回完整 SKILL.md 和可按需读取的资源路径。只能激活当前能力真实工具所引用的 Skill。",
            schema: schema(
                #"{"type":"object","properties":{"plugin_option":{"type":"string","minLength":1,"maxLength":80},"skill_name":{"type":"string","minLength":1,"maxLength":120}},"required":["plugin_option","skill_name"],"additionalProperties":false}"#
            ),
            providerID: ProductToolProviderID.capabilityBroker,
            skillBindingID: ProductToolSkillBindingID.capabilityBroker
        ),
        .init(
            name: readSkillResourceToolName,
            description: "分页读取已经激活的产品或固定 Plugin Skill 参考资料。只在当前决策需要对应场景、正反例或恢复细节时读取。",
            schema: schema(
                #"{"type":"object","properties":{"plugin_option":{"type":"string","minLength":1,"maxLength":80},"skill_name":{"type":"string","minLength":1,"maxLength":120},"relative_path":{"type":"string","minLength":1,"maxLength":500},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":64000}},"required":["plugin_option","skill_name","relative_path"],"additionalProperties":false}"#
            ),
            providerID: ProductToolProviderID.capabilityBroker,
            skillBindingID: ProductToolSkillBindingID.capabilityBroker
        ),
        .init(
            name: invokeToolName,
            description: "调用已经通过 capability_describe 展开的一个本机 Plugin 工具。若工具声明 required_skills，必须先逐个 capability_skill_activate；plugin_option 和 tool_option 使用本轮临时选项。",
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
        return .object([
            "type": .string("function"),
            "name": .string(definition.name),
            "description": .string(definition.description),
            "parameters": decoded,
        ])
    }

    private static func schema(_ value: String) -> Data { Data(value.utf8) }
}
