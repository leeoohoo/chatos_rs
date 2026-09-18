import Foundation

public enum LocalAgentCreationProposalStatus: String, Codable, Sendable {
    case pending, approved, rejected
}

/// Ordinary Agents can request a new teammate through Relay, but only the Human can resolve the
/// proposal. Account, project and room authority are supplied by the identity-bound MCP session.
public struct LocalAgentCreationProposal: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let roomID: String
    public let proposerAgentID: String
    public let sourceDeliveryID: String
    public let requestKey: String
    public let draft: LocalAgentDraft
    public let status: LocalAgentCreationProposalStatus
    public let createdAgentID: String?
    public let createdAtUnixMs: Int64
    public let resolvedAtUnixMs: Int64?

    public init(
        id: String,
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentDraft,
        status: LocalAgentCreationProposalStatus = .pending,
        createdAgentID: String? = nil,
        createdAtUnixMs: Int64,
        resolvedAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        self.proposerAgentID = proposerAgentID
        self.sourceDeliveryID = sourceDeliveryID
        self.requestKey = requestKey
        self.draft = draft
        self.status = status
        self.createdAgentID = createdAgentID
        self.createdAtUnixMs = createdAtUnixMs
        self.resolvedAtUnixMs = resolvedAtUnixMs
    }

    public func validate() throws {
        for (value, field) in [
            (id, "id"), (ownerUserID, "ownerUserID"), (roomID, "roomID"),
            (proposerAgentID, "proposerAgentID"), (sourceDeliveryID, "sourceDeliveryID"),
            (requestKey, "requestKey"),
        ] {
            try AgentGroupChatValidation.identifier(value, field: field)
        }
        try draft.validate()
        if let createdAgentID {
            try AgentGroupChatValidation.identifier(createdAgentID, field: "createdAgentID")
        }
        guard createdAtUnixMs >= 0,
              resolvedAtUnixMs == nil || resolvedAtUnixMs! >= createdAtUnixMs else {
            throw AgentGroupChatError.invalidField("proposalTimestamps")
        }
    }
}

public struct LocalAgentProposalApproval: Codable, Sendable, Equatable {
    public let proposal: LocalAgentCreationProposal
    public let agent: LocalAgentProfile
    public let member: ProjectAgentRoomMember?

    public init(
        proposal: LocalAgentCreationProposal,
        agent: LocalAgentProfile,
        member: ProjectAgentRoomMember? = nil
    ) {
        self.proposal = proposal
        self.agent = agent
        self.member = member
    }
}

/// High-risk capabilities are explicit Agent permissions, not Agent types. The values mirror
/// Relay's staffing permission names so prompts, MCP authorization and the management UI share
/// one vocabulary.
public enum LocalAgentPermission {
    public static let staffHire = "agent.staff.hire"
    public static let staffTerminate = "agent.staff.terminate"
    public static let localProjectList = "local.project.list"

    /// Profiles created by the first 3.0.3 preview used this role-like capability. Keep it only
    /// as a read-time compatibility marker; the editor normalizes it into explicit permissions.
    public static let legacyProjectSteward = "builtin.project-steward"

    public static func canManageStaff(_ permissions: [String]) -> Bool {
        let values = Set(permissions)
        return values.contains(legacyProjectSteward)
            || (values.contains(staffHire) && values.contains(staffTerminate))
    }

    public static func canAccessLocalProjects(_ permissions: [String]) -> Bool {
        let values = Set(permissions)
        return values.contains(legacyProjectSteward) || values.contains(localProjectList)
    }

    public static func normalized(
        preserving permissions: [String],
        canManageStaff: Bool,
        canAccessLocalProjects: Bool = false
    ) -> [String] {
        var values = Set(permissions)
        values.remove(legacyProjectSteward)
        values.remove(staffHire)
        values.remove(staffTerminate)
        values.remove(localProjectList)
        if canManageStaff {
            values.insert(staffHire)
            values.insert(staffTerminate)
        }
        if canAccessLocalProjects { values.insert(localProjectList) }
        return values.sorted()
    }
}

public enum LocalAgentRemovalProposalStatus: String, Codable, Sendable {
    case pending, approved, rejected
}

/// Removing a teammate is scoped to the current project team. The reusable Agent profile and its
/// private Memory remain intact; only a Human-approved proposal can change membership.
public struct LocalAgentRemovalProposalDraft: Codable, Sendable, Equatable {
    public let targetAgentID: String
    public let reason: String
    public let handoffPlan: String

    public init(targetAgentID: String, reason: String, handoffPlan: String = "") {
        self.targetAgentID = targetAgentID
        self.reason = reason
        self.handoffPlan = handoffPlan
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(targetAgentID, field: "targetAgentID")
        try AgentGroupChatValidation.text(reason, field: "reason", maximumLength: 4_000)
        try AgentGroupChatValidation.optionalText(
            handoffPlan,
            field: "handoffPlan",
            maximumLength: 8_000
        )
    }
}

public struct LocalAgentRemovalProposal: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let roomID: String
    public let proposerAgentID: String
    public let sourceDeliveryID: String
    public let requestKey: String
    public let draft: LocalAgentRemovalProposalDraft
    public let status: LocalAgentRemovalProposalStatus
    public let createdAtUnixMs: Int64
    public let resolvedAtUnixMs: Int64?

    public init(
        id: String,
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentRemovalProposalDraft,
        status: LocalAgentRemovalProposalStatus = .pending,
        createdAtUnixMs: Int64,
        resolvedAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        self.proposerAgentID = proposerAgentID
        self.sourceDeliveryID = sourceDeliveryID
        self.requestKey = requestKey
        self.draft = draft
        self.status = status
        self.createdAtUnixMs = createdAtUnixMs
        self.resolvedAtUnixMs = resolvedAtUnixMs
    }

    public func validate() throws {
        for (value, field) in [
            (id, "id"), (ownerUserID, "ownerUserID"), (roomID, "roomID"),
            (proposerAgentID, "proposerAgentID"), (sourceDeliveryID, "sourceDeliveryID"),
            (requestKey, "requestKey"),
        ] {
            try AgentGroupChatValidation.identifier(value, field: field)
        }
        try draft.validate()
        guard createdAtUnixMs >= 0,
              resolvedAtUnixMs == nil || resolvedAtUnixMs! >= createdAtUnixMs else {
            throw AgentGroupChatError.invalidField("removalProposalTimestamps")
        }
    }
}

public enum LocalAgentMembershipProposalStatus: String, Codable, Sendable {
    case pending, approved, rejected
}

/// A Human-approved request to attach an existing reusable Agent to a project team. Durable
/// identifiers are resolved from run-scoped opaque references inside the client and never need
/// to be supplied by the model.
public struct LocalAgentMembershipProposalDraft: Codable, Sendable, Equatable {
    public let targetTeamRoomID: String
    public let targetAgentID: String
    public let role: String
    public let responsibility: String

    public init(
        targetTeamRoomID: String,
        targetAgentID: String,
        role: String,
        responsibility: String = ""
    ) {
        self.targetTeamRoomID = targetTeamRoomID
        self.targetAgentID = targetAgentID
        self.role = role
        self.responsibility = responsibility
    }

    public func validate() throws {
        try AgentGroupChatValidation.identifier(targetTeamRoomID, field: "targetTeamRoomID")
        try AgentGroupChatValidation.identifier(targetAgentID, field: "targetAgentID")
        try AgentGroupChatValidation.text(role, field: "role", maximumLength: 160)
        try AgentGroupChatValidation.optionalText(
            responsibility,
            field: "responsibility",
            maximumLength: 8_000
        )
    }
}

public struct LocalAgentMembershipProposal: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let sourceRoomID: String
    public let proposerAgentID: String
    public let sourceDeliveryID: String
    public let requestKey: String
    public let draft: LocalAgentMembershipProposalDraft
    public let status: LocalAgentMembershipProposalStatus
    public let createdAtUnixMs: Int64
    public let resolvedAtUnixMs: Int64?

    public init(
        id: String,
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentMembershipProposalDraft,
        status: LocalAgentMembershipProposalStatus = .pending,
        createdAtUnixMs: Int64,
        resolvedAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.sourceRoomID = sourceRoomID
        self.proposerAgentID = proposerAgentID
        self.sourceDeliveryID = sourceDeliveryID
        self.requestKey = requestKey
        self.draft = draft
        self.status = status
        self.createdAtUnixMs = createdAtUnixMs
        self.resolvedAtUnixMs = resolvedAtUnixMs
    }

    public func validate() throws {
        for (value, field) in [
            (id, "id"), (ownerUserID, "ownerUserID"), (sourceRoomID, "sourceRoomID"),
            (proposerAgentID, "proposerAgentID"), (sourceDeliveryID, "sourceDeliveryID"),
            (requestKey, "requestKey"),
        ] {
            try AgentGroupChatValidation.identifier(value, field: field)
        }
        try draft.validate()
        guard createdAtUnixMs >= 0,
              resolvedAtUnixMs == nil || resolvedAtUnixMs! >= createdAtUnixMs else {
            throw AgentGroupChatError.invalidField("membershipProposalTimestamps")
        }
    }
}

public struct LocalAgentMembershipProposalApproval: Codable, Sendable, Equatable {
    public let proposal: LocalAgentMembershipProposal
    public let member: ProjectAgentRoomMember
    public let room: ProjectAgentRoom

    public init(
        proposal: LocalAgentMembershipProposal,
        member: ProjectAgentRoomMember,
        room: ProjectAgentRoom
    ) {
        self.proposal = proposal
        self.member = member
        self.room = room
    }
}

public enum LocalAgentTeamCreationProposalStatus: String, Codable, Sendable {
    case pending, approved, rejected
}

/// The project identifier is resolved from a run-scoped opaque option entirely inside the client,
/// then copied verbatim into this durable proposal. It never enters model input or output. Project
/// names remain display metadata and are never used to resolve the target project.
public struct LocalAgentTeamCreationProposalDraft: Codable, Sendable, Equatable {
    public let existingProjectID: String?
    public let newProjectName: String?
    public let newProjectDescription: String
    public let newProjectTypeKey: String?
    public let teamName: String
    public let teamGoal: String

    public init(
        existingProjectID: String,
        teamName: String,
        teamGoal: String = ""
    ) {
        self.existingProjectID = existingProjectID
        newProjectName = nil
        newProjectDescription = ""
        newProjectTypeKey = nil
        self.teamName = teamName
        self.teamGoal = teamGoal
    }

    public init(
        newProjectName: String,
        newProjectDescription: String = "",
        newProjectTypeKey: String = LocalAgentSkillCatalog.legacyProjectTypeKey,
        teamName: String,
        teamGoal: String = ""
    ) {
        existingProjectID = nil
        self.newProjectName = newProjectName
        self.newProjectDescription = newProjectDescription
        self.newProjectTypeKey = newProjectTypeKey
        self.teamName = teamName
        self.teamGoal = teamGoal
    }

    public func validate() throws {
        guard (existingProjectID == nil) != (newProjectName == nil) else {
            throw AgentGroupChatError.invalidField("projectSelection")
        }
        if let existingProjectID {
            try AgentGroupChatValidation.identifier(existingProjectID, field: "projectID")
            guard newProjectDescription.isEmpty else {
                throw AgentGroupChatError.invalidField("newProjectDescription")
            }
            guard newProjectTypeKey == nil else {
                throw AgentGroupChatError.invalidField("newProjectTypeKey")
            }
        }
        if let newProjectName {
            try AgentGroupChatValidation.text(
                newProjectName,
                field: "newProjectName",
                maximumLength: 160
            )
            try AgentGroupChatValidation.optionalText(
                newProjectDescription,
                field: "newProjectDescription",
                maximumLength: 8_000
            )
            if let newProjectTypeKey {
                do {
                    _ = try LocalAgentSkillCatalog.requireProjectType(key: newProjectTypeKey)
                } catch {
                    throw AgentGroupChatError.invalidField("newProjectTypeKey")
                }
            }
        }
        try AgentGroupChatValidation.text(teamName, field: "teamName", maximumLength: 160)
        try AgentGroupChatValidation.optionalText(teamGoal, field: "teamGoal", maximumLength: 8_000)
    }
}

public struct LocalAgentTeamCreationProposal: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let sourceRoomID: String
    public let proposerAgentID: String
    public let sourceDeliveryID: String
    public let requestKey: String
    public let draft: LocalAgentTeamCreationProposalDraft
    public let status: LocalAgentTeamCreationProposalStatus
    public let createdRoomID: String?
    public let createdAtUnixMs: Int64
    public let resolvedAtUnixMs: Int64?

    public init(
        id: String,
        ownerUserID: String,
        sourceRoomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalAgentTeamCreationProposalDraft,
        status: LocalAgentTeamCreationProposalStatus = .pending,
        createdRoomID: String? = nil,
        createdAtUnixMs: Int64,
        resolvedAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.sourceRoomID = sourceRoomID
        self.proposerAgentID = proposerAgentID
        self.sourceDeliveryID = sourceDeliveryID
        self.requestKey = requestKey
        self.draft = draft
        self.status = status
        self.createdRoomID = createdRoomID
        self.createdAtUnixMs = createdAtUnixMs
        self.resolvedAtUnixMs = resolvedAtUnixMs
    }

    public func validate() throws {
        for (value, field) in [
            (id, "id"), (ownerUserID, "ownerUserID"), (sourceRoomID, "sourceRoomID"),
            (proposerAgentID, "proposerAgentID"), (sourceDeliveryID, "sourceDeliveryID"),
            (requestKey, "requestKey"),
        ] {
            try AgentGroupChatValidation.identifier(value, field: field)
        }
        try draft.validate()
        if let createdRoomID {
            try AgentGroupChatValidation.identifier(createdRoomID, field: "createdRoomID")
        }
        guard createdAtUnixMs >= 0,
              resolvedAtUnixMs == nil || resolvedAtUnixMs! >= createdAtUnixMs else {
            throw AgentGroupChatError.invalidField("teamProposalTimestamps")
        }
    }
}

public struct LocalAgentTeamProposalApproval: Codable, Sendable, Equatable {
    public let proposal: LocalAgentTeamCreationProposal
    public let room: ProjectAgentRoom

    public init(proposal: LocalAgentTeamCreationProposal, room: ProjectAgentRoom) {
        self.proposal = proposal
        self.room = room
    }
}

public enum LocalProjectCreationProposalStatus: String, Codable, Sendable {
    case pending, approved, rejected
}

/// A project proposal contains only host-independent metadata. The Agent cannot choose or infer
/// an absolute directory; the Human selects an authorized local workspace when confirming it.
public struct LocalProjectCreationProposalDraft: Codable, Sendable, Equatable {
    public let name: String
    public let description: String
    public let projectTypeKey: String

    public init(
        name: String,
        description: String = "",
        projectTypeKey: String = LocalAgentSkillCatalog.legacyProjectTypeKey
    ) {
        self.name = name
        self.description = description
        self.projectTypeKey = projectTypeKey
    }

    public func validate() throws {
        try ProjectRegistryValidation.identifier(name, field: "projectProposal.name")
        guard name.count <= 160,
              description.count <= 8_000,
              !description.contains("\0") else {
            throw AgentGroupChatError.invalidField("projectProposal")
        }
        do {
            _ = try LocalAgentSkillCatalog.requireProjectType(key: projectTypeKey)
        } catch {
            throw AgentGroupChatError.invalidField("projectTypeKey")
        }
    }

    private enum CodingKeys: String, CodingKey { case name, description, projectTypeKey }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        self.init(
            name: try values.decode(String.self, forKey: .name),
            description: try values.decodeIfPresent(String.self, forKey: .description) ?? "",
            projectTypeKey: try values.decodeIfPresent(String.self, forKey: .projectTypeKey)
                ?? LocalAgentSkillCatalog.legacyProjectTypeKey
        )
    }
}

public struct LocalProjectCreationProposal: Codable, Sendable, Equatable, Identifiable {
    public let id: String
    public let ownerUserID: String
    public let roomID: String
    public let proposerAgentID: String
    public let sourceDeliveryID: String
    public let requestKey: String
    public let draft: LocalProjectCreationProposalDraft
    public let status: LocalProjectCreationProposalStatus
    public let createdProjectID: String?
    public let createdAtUnixMs: Int64
    public let resolvedAtUnixMs: Int64?

    public init(
        id: String,
        ownerUserID: String,
        roomID: String,
        proposerAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        draft: LocalProjectCreationProposalDraft,
        status: LocalProjectCreationProposalStatus = .pending,
        createdProjectID: String? = nil,
        createdAtUnixMs: Int64,
        resolvedAtUnixMs: Int64? = nil
    ) {
        self.id = id
        self.ownerUserID = ownerUserID
        self.roomID = roomID
        self.proposerAgentID = proposerAgentID
        self.sourceDeliveryID = sourceDeliveryID
        self.requestKey = requestKey
        self.draft = draft
        self.status = status
        self.createdProjectID = createdProjectID
        self.createdAtUnixMs = createdAtUnixMs
        self.resolvedAtUnixMs = resolvedAtUnixMs
    }

    public func validate() throws {
        for (value, field) in [
            (id, "id"), (ownerUserID, "ownerUserID"), (roomID, "roomID"),
            (proposerAgentID, "proposerAgentID"), (sourceDeliveryID, "sourceDeliveryID"),
            (requestKey, "requestKey"),
        ] {
            try AgentGroupChatValidation.identifier(value, field: field)
        }
        try draft.validate()
        if let createdProjectID {
            try AgentGroupChatValidation.identifier(createdProjectID, field: "createdProjectID")
        }
        guard createdAtUnixMs >= 0,
              resolvedAtUnixMs == nil || resolvedAtUnixMs! >= createdAtUnixMs else {
            throw AgentGroupChatError.invalidField("projectProposalTimestamps")
        }
    }
}
