import ChatOSCore

/// Native teardown adapter for the Local Agent Application shutdown use case.
public struct NativeLocalAgentServiceShutdownAdapter: LocalAgentServiceShutdownPort {
    private let commandService: NativeLocalAgentConversationService?
    private let petActivityService: NativeLocalAgentPetActivityService?
    private let messageTaskGraphService: NativeLocalAgentMessageTaskGraphService?
    private let askUserPromptService: NativeLocalAgentAskUserPromptService?
    private let turnProcessService: NativeLocalAgentTurnProcessService?
    private let runtimeSettingsService: NativeLocalAgentConversationRuntimeSettingsService?
    private let platformToolWorker: NativeLocalAgentPlatformToolWorker?
    private let workspaceService: NativeLocalAgentWorkspaceService?
    private let projectConversationService: NativeLocalAgentProjectConversationService?
    private let notepadService: NativeLocalAgentNotepadService
    private let remoteConnectionMetadataService: NativeLocalAgentRemoteConnectionMetadataService
    private let host: any LocalAgentHostLifecycleServicing

    public init(
        commandService: NativeLocalAgentConversationService?,
        petActivityService: NativeLocalAgentPetActivityService?,
        messageTaskGraphService: NativeLocalAgentMessageTaskGraphService?,
        askUserPromptService: NativeLocalAgentAskUserPromptService?,
        turnProcessService: NativeLocalAgentTurnProcessService?,
        runtimeSettingsService: NativeLocalAgentConversationRuntimeSettingsService?,
        platformToolWorker: NativeLocalAgentPlatformToolWorker?,
        workspaceService: NativeLocalAgentWorkspaceService?,
        projectConversationService: NativeLocalAgentProjectConversationService?,
        notepadService: NativeLocalAgentNotepadService,
        remoteConnectionMetadataService: NativeLocalAgentRemoteConnectionMetadataService,
        host: any LocalAgentHostLifecycleServicing
    ) {
        self.commandService = commandService
        self.petActivityService = petActivityService
        self.messageTaskGraphService = messageTaskGraphService
        self.askUserPromptService = askUserPromptService
        self.turnProcessService = turnProcessService
        self.runtimeSettingsService = runtimeSettingsService
        self.platformToolWorker = platformToolWorker
        self.workspaceService = workspaceService
        self.projectConversationService = projectConversationService
        self.notepadService = notepadService
        self.remoteConnectionMetadataService = remoteConnectionMetadataService
        self.host = host
    }

    public func reset(_ stage: LocalAgentServiceShutdownStage) async {
        switch stage {
        case .conversationCommand:
            await commandService?.reset()
        case .petActivity:
            await petActivityService?.reset()
        case .messageTaskGraph:
            await messageTaskGraphService?.reset()
        case .askUserPrompt:
            await askUserPromptService?.reset()
        case .turnProcess:
            await turnProcessService?.reset()
        case .runtimeSettings:
            await runtimeSettingsService?.reset()
        case .platformToolWorker:
            await platformToolWorker?.reset()
        case .workspace:
            await workspaceService?.reset()
        case .projectConversation:
            await projectConversationService?.reset()
        case .notepad:
            await notepadService.reset()
        case .remoteConnectionMetadata:
            await remoteConnectionMetadataService.reset()
        case .host:
            await host.stop()
        }
    }
}
