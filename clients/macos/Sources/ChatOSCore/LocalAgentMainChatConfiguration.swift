// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation

public struct LocalAgentContactSkill: Sendable, Equatable {
    public var id: String
    public var name: String
    public var content: String

    public init(id: String, name: String, content: String) {
        self.id = id
        self.name = name
        self.content = content
    }
}

public struct LocalAgentContactRuntimeContext: Sendable, Equatable {
    public var agentID: String
    public var name: String
    public var description: String?
    public var category: String?
    public var roleDefinition: String
    public var skills: [LocalAgentContactSkill]
    public var revision: String

    public init(
        agentID: String,
        name: String,
        description: String?,
        category: String?,
        roleDefinition: String,
        skills: [LocalAgentContactSkill],
        revision: String
    ) {
        self.agentID = agentID
        self.name = name
        self.description = description
        self.category = category
        self.roleDefinition = roleDefinition
        self.skills = skills
        self.revision = revision
    }
}

public protocol LocalAgentContactRuntimeContextServicing: Sendable {
    func fetchRuntimeContext(agentID: String) async throws -> LocalAgentContactRuntimeContext
}
