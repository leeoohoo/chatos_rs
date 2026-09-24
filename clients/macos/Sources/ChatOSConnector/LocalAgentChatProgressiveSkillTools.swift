import ChatOSAgentRuntime
import ChatOSCore
import Foundation

actor LocalAgentProgressiveSkillSession {
    private let snapshot: LocalAgentProgressiveSkillSnapshot?
    private var activatedRefs: Set<String> = []

    init(snapshot: LocalAgentProgressiveSkillSnapshot?) {
        self.snapshot = snapshot
    }

    var hasSkills: Bool { snapshot?.skills.isEmpty == false }

    func activate(skillRef: String) throws -> LocalAgentBoundProgressiveSkill {
        let skill = try boundSkill(skillRef)
        activatedRefs.insert(skillRef)
        return skill
    }

    func resources(skillRef: String) throws -> [LocalAgentProgressiveSkillResource] {
        guard activatedRefs.contains(skillRef) else {
            throw AgentGroupChatError.invalidField("skill_ref_not_activated")
        }
        return try boundSkill(skillRef).resources
    }

    func read(
        skillRef: String,
        relativePath: String,
        offset: Int,
        maximumCharacters: Int
    ) throws -> SkillResourcePage {
        guard activatedRefs.contains(skillRef) else {
            throw AgentGroupChatError.invalidField("skill_ref_not_activated")
        }
        let normalized = try ProgressiveSkillFileLoader.normalizedRelativePath(relativePath)
        guard let resource = try boundSkill(skillRef).resources.first(where: {
            $0.relativePath == normalized
        }) else { throw AgentGroupChatError.notFound }
        let page: ProgressiveSkillFileLoader.TextPage
        do {
            page = try ProgressiveSkillFileLoader.textPage(
                resource.markdown,
                offset: offset,
                maximumCharacters: maximumCharacters
            )
        } catch ProgressiveSkillFileLoader.LoaderError.invalidOffset {
            throw AgentGroupChatError.invalidField("offset")
        }
        return .init(
            resource: resource,
            content: page.content,
            offset: page.offset,
            nextOffset: page.nextOffset,
            truncated: page.truncated
        )
    }

    private func boundSkill(_ skillRef: String) throws -> LocalAgentBoundProgressiveSkill {
        guard let skill = snapshot?.skills.first(where: { $0.skillRef == skillRef }) else {
            // The model cannot enumerate or switch into another catalog entry.
            throw AgentGroupChatError.invalidField("skill_ref")
        }
        return skill
    }
}

struct SkillResourcePage: Sendable {
    let resource: LocalAgentProgressiveSkillResource
    let content: String
    let offset: Int
    let nextOffset: Int?
    let truncated: Bool
}

extension LocalAgentChatToolProvider {
    struct ProgressiveSkillResourceResponse: Encodable {
        let relativePath: String
        let title: String
        let summary: String
        let sizeBytes: Int
        let sha256: String

        enum CodingKeys: String, CodingKey {
            case relativePath = "relative_path"
            case title, summary
            case sizeBytes = "size_bytes"
            case sha256
        }

        init(_ value: LocalAgentProgressiveSkillResource) {
            relativePath = value.relativePath
            title = value.title
            summary = value.summary
            sizeBytes = value.sizeBytes
            sha256 = value.contentSHA256
        }
    }

    struct ProgressiveSkillActivationResponse: Encodable {
        let activated = true
        let skillRef: String
        let kind: String
        let name: String
        let label: String
        let description: String
        let category: String
        let instructions: String
        let instructionsSHA256: String
        let resources: [ProgressiveSkillResourceResponse]

        enum CodingKeys: String, CodingKey {
            case activated
            case skillRef = "skill_ref"
            case kind, name, label, description, category, instructions
            case instructionsSHA256 = "instructions_sha256"
            case resources
        }
    }

    struct ProgressiveSkillResourceListResponse: Encodable {
        let skillRef: String
        let resources: [ProgressiveSkillResourceResponse]

        enum CodingKeys: String, CodingKey {
            case skillRef = "skill_ref"
            case resources
        }
    }

    struct ProgressiveSkillResourcePageResponse: Encodable {
        let skillRef: String
        let relativePath: String
        let sha256: String
        let content: String
        let offset: Int
        let nextOffset: Int?
        let truncated: Bool

        enum CodingKeys: String, CodingKey {
            case skillRef = "skill_ref"
            case relativePath = "relative_path"
            case sha256, content, offset
            case nextOffset = "next_offset"
            case truncated
        }
    }

    func activateAgentSkill(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let skill = try await progressiveSkills.activate(
            skillRef: Self.requiredString(arguments, key: "skill_ref")
        )
        return try Self.outcome(ProgressiveSkillActivationResponse(
            skillRef: skill.skillRef,
            kind: skill.kind.rawValue,
            name: skill.name,
            label: skill.label,
            description: skill.description,
            category: skill.category,
            instructions: skill.instructions,
            instructionsSHA256: skill.instructionsSHA256,
            resources: skill.resources.map(ProgressiveSkillResourceResponse.init)
        ))
    }

    func listAgentSkillResources(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let skillRef = try Self.requiredString(arguments, key: "skill_ref")
        let resources = try await progressiveSkills.resources(skillRef: skillRef)
        return try Self.outcome(ProgressiveSkillResourceListResponse(
            skillRef: skillRef,
            resources: resources.map(ProgressiveSkillResourceResponse.init)
        ))
    }

    func readAgentSkillResource(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        let arguments = try Self.arguments(call)
        let skillRef = try Self.requiredString(arguments, key: "skill_ref")
        let path = try Self.requiredString(arguments, key: "relative_path")
        let offset = Int(try Self.optionalInteger(arguments, key: "offset") ?? 0)
        let maxChars = Int(try Self.optionalInteger(arguments, key: "max_chars") ?? 32_000)
        let page = try await progressiveSkills.read(
            skillRef: skillRef,
            relativePath: path,
            offset: offset,
            maximumCharacters: maxChars
        )
        return try Self.outcome(ProgressiveSkillResourcePageResponse(
            skillRef: skillRef,
            relativePath: page.resource.relativePath,
            sha256: page.resource.contentSHA256,
            content: page.content,
            offset: page.offset,
            nextOffset: page.nextOffset,
            truncated: page.truncated
        ))
    }
}
