import ChatOSCore

struct PluginSummary: Encodable {
    let pluginOption: String
    let name: String
    let description: String
}

struct SearchResponse: Encodable {
    let matches: [PluginSummary]
}

struct ToolSummary: Encodable {
    let toolOption: String
    let name: String
    let description: String
    let inputSchema: NativeJSONValue
    let effect: String
    let requiredSkills: [String]
}

struct SkillSummary: Encodable {
    let name: String
    let role: String
    let description: String
}

struct DescribeResponse: Encodable {
    let pluginOption: String
    let name: String
    let tools: [ToolSummary]
    let skills: [SkillSummary]
}

struct SkillActivationResponse: Encodable {
    let pluginOption: String
    let skillName: String
    let instructions: String
    let resources: [String]
}

struct SkillResourceResponse: Encodable {
    let pluginOption: String
    let skillName: String
    let relativePath: String
    let content: String
    let offset: Int
    let nextOffset: Int?
    let truncated: Bool
}
