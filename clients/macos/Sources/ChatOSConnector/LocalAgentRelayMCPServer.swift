import ChatOSAgentRuntime
import ChatOSCore
import Foundation

/// Safe catalog metadata supplied by the client. `pluginID` remains inside the provider and is
/// mapped from a run-scoped opaque reference before a Todo execution plan is persisted.
public struct LocalAgentTodoPluginOption: Sendable, Equatable {
    public let pluginID: String
    public let displayName: String
    public let description: String

    public init(pluginID: String, displayName: String, description: String) {
        self.pluginID = pluginID
        self.displayName = displayName
        self.description = description
    }
}

/// Immutable authority for one claimed local delivery. Caller-controlled tool arguments never
/// select the sender, account, project, room or Memory identity.
public struct LocalAgentChatRunContext: Codable, Sendable, Equatable {
    public let ownerUserID: String
    public let projectID: String
    public let roomID: String
    public let agentID: String
    public let deliveryID: String
    public let triggerMessageID: String
    public let rootMessageID: String
    public let runID: String
    public let hopCount: Int
    public let lane: LocalAgentRunLane

    public init(
        ownerUserID: String,
        projectID: String,
        roomID: String,
        agentID: String,
        deliveryID: String,
        triggerMessageID: String,
        rootMessageID: String,
        runID: String,
        hopCount: Int,
        lane: LocalAgentRunLane = .manager
    ) throws {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(projectID, field: "projectID")
        try AgentGroupChatValidation.identifier(roomID, field: "roomID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(deliveryID, field: "deliveryID")
        try AgentGroupChatValidation.identifier(triggerMessageID, field: "triggerMessageID")
        try AgentGroupChatValidation.identifier(rootMessageID, field: "rootMessageID")
        try AgentGroupChatValidation.identifier(runID, field: "runID")
        guard (0...64).contains(hopCount) else {
            throw AgentGroupChatError.invalidField("hopCount")
        }
        self.ownerUserID = ownerUserID
        self.projectID = projectID
        self.roomID = roomID
        self.agentID = agentID
        self.deliveryID = deliveryID
        self.triggerMessageID = triggerMessageID
        self.rootMessageID = rootMessageID
        self.runID = runID
        self.hopCount = hopCount
        self.lane = lane
    }

    private enum CodingKeys: String, CodingKey {
        case ownerUserID, projectID, roomID, agentID, deliveryID, triggerMessageID
        case rootMessageID, runID, hopCount, lane
    }

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        try self.init(
            ownerUserID: values.decode(String.self, forKey: .ownerUserID),
            projectID: values.decode(String.self, forKey: .projectID),
            roomID: values.decode(String.self, forKey: .roomID),
            agentID: values.decode(String.self, forKey: .agentID),
            deliveryID: values.decode(String.self, forKey: .deliveryID),
            triggerMessageID: values.decode(String.self, forKey: .triggerMessageID),
            rootMessageID: values.decode(String.self, forKey: .rootMessageID),
            runID: values.decode(String.self, forKey: .runID),
            hopCount: values.decode(Int.self, forKey: .hopCount),
            lane: values.decodeIfPresent(LocalAgentRunLane.self, forKey: .lane) ?? .manager
        )
    }
}

/// The single local collaboration MCP shared by every native Agent. It owns no model runtime:
/// each connection is scoped to one immutable Agent delivery identity and all communication
/// goes through the same durable room store. A future stdio/HTTP transport can delegate to this
/// server without changing its authorization or routing semantics.
public actor LocalAgentRelayMCPServer {
    private let service: NativeAgentGroupChatService
    private let limits: AgentGroupChatRoutingLimits
    private let now: @Sendable () -> Int64
    private let todoCancellationHandler: @Sendable (String) async -> Void

    public init(
        service: NativeAgentGroupChatService,
        limits: AgentGroupChatRoutingLimits = .init(),
        todoCancellationHandler: @escaping @Sendable (String) async -> Void = { _ in },
        now: @escaping @Sendable () -> Int64 = {
            Int64(Date().timeIntervalSince1970 * 1_000)
        }
    ) {
        self.service = service
        self.limits = limits
        self.todoCancellationHandler = todoCancellationHandler
        self.now = now
    }

    public func connect(
        context: LocalAgentChatRunContext,
        professions: [LocalAgentProfessionDefinition] = LocalAgentSkillCatalog.professions,
        progressiveSkillSnapshot: LocalAgentProgressiveSkillSnapshot? = nil,
        todoPluginOptions: [LocalAgentTodoPluginOption] = [],
        productSkillSession: ProductToolSkillSession = .init()
    ) async throws -> LocalAgentChatToolProvider {
        let store = try await service.store()
        let documentDraftDirectoryURL = try await store.createAgentDocumentDraftDirectory()
        return try LocalAgentChatToolProvider(
            store: store,
            context: context,
            professions: professions,
            progressiveSkillSnapshot: progressiveSkillSnapshot,
            productSkillSession: productSkillSession,
            todoPluginOptions: todoPluginOptions,
            limits: limits,
            todoCancellationHandler: todoCancellationHandler,
            roomChangeHandler: { [service] roomID in
                await service.publishChange(.init(
                    ownerUserID: context.ownerUserID,
                    roomID: roomID,
                    agentID: context.agentID,
                    kind: .roomUpdated
                ))
            },
            documentDraftDirectoryURL: documentDraftDirectoryURL,
            now: now
        )
    }
}
