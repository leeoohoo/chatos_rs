import ChatOSCore

/// Builds the native adapter used by the Application service-configuration use case.
///
/// Connector bootstrap details and transient external MCP credentials remain below the port;
/// neither the Application use case nor AppModel needs to understand them.
public struct NativeLocalAgentServiceConfigurationFactory: Sendable {
    private let localConnectorService: NativeLocalConnectorService
    private let runtimeSettingsService: NativeLocalAgentConversationRuntimeSettingsService?
    private let petActivityService: NativeLocalAgentPetActivityService?
    private let commandService: NativeLocalAgentConversationService?
    private let messageTaskGraphService: NativeLocalAgentMessageTaskGraphService?
    private let askUserPromptService: NativeLocalAgentAskUserPromptService?
    private let turnProcessService: NativeLocalAgentTurnProcessService?
    private let platformToolWorker: NativeLocalAgentPlatformToolWorker?

    public init(
        localConnectorService: NativeLocalConnectorService,
        runtimeSettingsService: NativeLocalAgentConversationRuntimeSettingsService?,
        petActivityService: NativeLocalAgentPetActivityService?,
        commandService: NativeLocalAgentConversationService?,
        messageTaskGraphService: NativeLocalAgentMessageTaskGraphService?,
        askUserPromptService: NativeLocalAgentAskUserPromptService?,
        turnProcessService: NativeLocalAgentTurnProcessService?,
        platformToolWorker: NativeLocalAgentPlatformToolWorker?
    ) {
        self.localConnectorService = localConnectorService
        self.runtimeSettingsService = runtimeSettingsService
        self.petActivityService = petActivityService
        self.commandService = commandService
        self.messageTaskGraphService = messageTaskGraphService
        self.askUserPromptService = askUserPromptService
        self.turnProcessService = turnProcessService
        self.platformToolWorker = platformToolWorker
    }

    public func makePort(
        ownerUserID: String,
        bootstrap: NativeLocalAgentBootstrapResult
    ) -> any LocalAgentServiceConfigurationPort {
        NativeLocalAgentServiceConfigurationAdapter(
            ownerUserID: ownerUserID,
            bootstrap: bootstrap,
            localConnectorService: localConnectorService,
            runtimeSettingsService: runtimeSettingsService,
            petActivityService: petActivityService,
            commandService: commandService,
            messageTaskGraphService: messageTaskGraphService,
            askUserPromptService: askUserPromptService,
            turnProcessService: turnProcessService,
            platformToolWorker: platformToolWorker
        )
    }
}

private struct NativeLocalAgentServiceConfigurationAdapter: LocalAgentServiceConfigurationPort {
    let ownerUserID: String
    let bootstrap: NativeLocalAgentBootstrapResult
    let localConnectorService: NativeLocalConnectorService
    let runtimeSettingsService: NativeLocalAgentConversationRuntimeSettingsService?
    let petActivityService: NativeLocalAgentPetActivityService?
    let commandService: NativeLocalAgentConversationService?
    let messageTaskGraphService: NativeLocalAgentMessageTaskGraphService?
    let askUserPromptService: NativeLocalAgentAskUserPromptService?
    let turnProcessService: NativeLocalAgentTurnProcessService?
    let platformToolWorker: NativeLocalAgentPlatformToolWorker?

    func configure(_ stage: LocalAgentServiceConfigurationStage) async throws {
        switch stage {
        case .localApprovalAgent:
            await localConnectorService.configureLocalApprovalAgent(
                ownerUserID: ownerUserID,
                bootstrap: bootstrap
            )
        case .runtimeSettings:
            try await runtimeSettingsService?.configure(
                ownerUserID: ownerUserID,
                bootstrap: bootstrap
            )
        case .petActivity:
            await petActivityService?.configure(ownerUserID: ownerUserID)
        case .conversationCommand:
            try await commandService?.configure(
                ownerUserID: ownerUserID,
                bootstrap: bootstrap
            )
        case .messageTaskGraph:
            await messageTaskGraphService?.configure(ownerUserID: ownerUserID)
        case .askUserPrompt:
            await askUserPromptService?.configure(ownerUserID: ownerUserID)
        case .turnProcess:
            await turnProcessService?.configure(ownerUserID: ownerUserID)
        case .platformToolWorker:
            try await platformToolWorker?.configure(
                ownerUserID: ownerUserID,
                externalMCPConfigs: bootstrap.externalMCPConfigs
            )
        }
    }
}
