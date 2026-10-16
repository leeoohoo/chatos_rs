import Foundation

public enum LocalAgentToolSkillActivationPolicy: String, Codable, Sendable, Equatable {
    case onDemand = "on_demand"
    case runBound = "run_bound"
    case controlPlane = "control_plane"
}

public struct LocalAgentSkillResourceSnapshot: Codable, Sendable, Equatable {
    public let relativePath: String
    public let content: String
    public let sha256: String

    public init(relativePath: String, content: String, sha256: String) {
        self.relativePath = relativePath
        self.content = content
        self.sha256 = sha256
    }

    private enum CodingKeys: String, CodingKey {
        case content, sha256
        case relativePath = "relative_path"
    }
}

public struct LocalAgentSkillDocumentSnapshot: Codable, Sendable, Equatable {
    public let skillRef: String
    public let name: String
    public let role: String
    public let description: String
    public let instructions: String
    public let instructionsSHA256: String
    public let resources: [LocalAgentSkillResourceSnapshot]

    public init(
        skillRef: String,
        name: String,
        role: String,
        description: String,
        instructions: String,
        instructionsSHA256: String,
        resources: [LocalAgentSkillResourceSnapshot]
    ) {
        self.skillRef = skillRef
        self.name = name
        self.role = role
        self.description = description
        self.instructions = instructions
        self.instructionsSHA256 = instructionsSHA256
        self.resources = resources
    }

    private enum CodingKeys: String, CodingKey {
        case name, role, description, instructions, resources
        case skillRef = "skill_ref"
        case instructionsSHA256 = "instructions_sha256"
    }
}

public struct LocalAgentToolSkillBindingSnapshot: Codable, Sendable, Equatable {
    public let providerID: String
    public let bindingID: String
    public let toolNames: [String]
    public let routerSkillRef: String
    public let specialistSkillRef: String
    public let activationPolicy: LocalAgentToolSkillActivationPolicy

    public init(
        providerID: String,
        bindingID: String,
        toolNames: [String],
        routerSkillRef: String,
        specialistSkillRef: String,
        activationPolicy: LocalAgentToolSkillActivationPolicy
    ) {
        self.providerID = providerID
        self.bindingID = bindingID
        self.toolNames = toolNames
        self.routerSkillRef = routerSkillRef
        self.specialistSkillRef = specialistSkillRef
        self.activationPolicy = activationPolicy
    }

    private enum CodingKeys: String, CodingKey {
        case providerID = "provider_id"
        case bindingID = "binding_id"
        case toolNames = "tool_names"
        case routerSkillRef = "router_skill_ref"
        case specialistSkillRef = "specialist_skill_ref"
        case activationPolicy = "activation_policy"
    }
}

public struct LocalAgentSkillBundleSnapshot: Codable, Sendable, Equatable {
    public let schemaVersion: UInt32
    public let bundleRevision: String
    public let documents: [LocalAgentSkillDocumentSnapshot]
    public let toolSkillBindings: [LocalAgentToolSkillBindingSnapshot]

    public init(
        schemaVersion: UInt32 = 1,
        bundleRevision: String,
        documents: [LocalAgentSkillDocumentSnapshot],
        toolSkillBindings: [LocalAgentToolSkillBindingSnapshot]
    ) {
        self.schemaVersion = schemaVersion
        self.bundleRevision = bundleRevision
        self.documents = documents
        self.toolSkillBindings = toolSkillBindings
    }

    private enum CodingKeys: String, CodingKey {
        case documents
        case schemaVersion = "schema_version"
        case bundleRevision = "bundle_revision"
        case toolSkillBindings = "tool_skill_bindings"
    }
}
