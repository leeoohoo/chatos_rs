// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public struct ChatOSLocalAgentContactRuntimeContextService:
    LocalAgentContactRuntimeContextServicing
{
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func fetchRuntimeContext(
        agentID: String
    ) async throws -> LocalAgentContactRuntimeContext {
        let response: RuntimeContextDTO = try await client.request(
            "/agents/\(agentID.urlPathEncoded)/runtime-context"
        )
        return LocalAgentContactRuntimeContext(
            agentID: response.agentID,
            name: response.name,
            description: response.description?.trimmedNonEmptyValue,
            category: response.category?.trimmedNonEmptyValue,
            roleDefinition: response.roleDefinition,
            skills: response.skills.map {
                LocalAgentContactSkill(id: $0.id, name: $0.name, content: $0.content)
            },
            revision: response.updatedAt
        )
    }
}

private struct RuntimeContextDTO: Decodable, Sendable {
    var agentID: String
    var name: String
    var description: String?
    var category: String?
    var roleDefinition: String
    var skills: [SkillDTO]
    var updatedAt: String

    enum CodingKeys: String, CodingKey {
        case name, description, category, skills
        case agentID = "agent_id"
        case roleDefinition = "role_definition"
        case updatedAt = "updated_at"
    }
}

private struct SkillDTO: Decodable, Sendable {
    var id: String
    var name: String
    var content: String
}
