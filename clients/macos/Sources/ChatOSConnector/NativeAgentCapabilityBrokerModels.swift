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
    let inputSchema: NativeJSONValue?
    let effect: String
    let requiredSkillRefs: [String]
    // Task built-ins execute through the durable native Task tool path.
    let invocationMode: String
    let skillGate: NativeJSONValue
}

struct SkillSummary: Encodable {
    let skillRef: String
    let name: String
    let role: String
    let description: String
}

struct DescribeResponse: Encodable {
    let bindingSchemaVersion: Int
    let providerID: String
    let pluginOption: String
    let name: String
    let tools: [ToolSummary]
    let skills: [SkillSummary]
}

struct SkillActivationResponse: Encodable {
    let skillRef: String
    let instructions: String
    let resources: [String]
}

struct SkillResourceResponse: Encodable {
    let skillRef: String
    let relativePath: String
    let content: String
    let offset: Int
    let nextOffset: Int?
    let truncated: Bool
}
