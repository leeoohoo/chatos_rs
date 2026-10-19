import ChatOSCore
import Foundation

/// App-layer composition boundary for a conversation presentation session.
///
/// AppModel owns cache and activation policy. This factory owns the dependency mapping
/// from stable Core ports to the SwiftUI adapter so individual callers cannot construct
/// partially configured conversation sessions.
@MainActor
struct ConversationSessionFactory {
    private let historyStore: any ConversationHistoryStoring
    private let remoteService: (any ConversationRemoteServicing)?
    private let realtimeService: (any ConversationRealtimeStreaming)?
    private let commandService: (any ConversationCommandServicing)?
    private let turnProcessService: (any TurnProcessServicing)?
    private let messageTaskGraphService: (any MessageTaskGraphServicing)?
    private let runtimeSettingsService: (any ConversationRuntimeSettingsServicing)?
    private let askUserPromptService: (any AskUserPromptServicing)?

    init(
        historyStore: any ConversationHistoryStoring,
        remoteService: (any ConversationRemoteServicing)? = nil,
        realtimeService: (any ConversationRealtimeStreaming)? = nil,
        commandService: (any ConversationCommandServicing)? = nil,
        turnProcessService: (any TurnProcessServicing)? = nil,
        messageTaskGraphService: (any MessageTaskGraphServicing)? = nil,
        runtimeSettingsService: (any ConversationRuntimeSettingsServicing)? = nil,
        askUserPromptService: (any AskUserPromptServicing)? = nil
    ) {
        self.historyStore = historyStore
        self.remoteService = remoteService
        self.realtimeService = realtimeService
        self.commandService = commandService
        self.turnProcessService = turnProcessService
        self.messageTaskGraphService = messageTaskGraphService
        self.runtimeSettingsService = runtimeSettingsService
        self.askUserPromptService = askUserPromptService
    }

    func makeSession(
        sessionID: String,
        initialTurns: [ConversationTurn] = [],
        localAgentRuntimeReady: Bool
    ) -> ConversationSessionViewModel {
        ConversationSessionViewModel(
            sessionID: sessionID,
            initialTurns: initialTurns,
            historyStore: historyStore,
            localAgentRuntimeReady: localAgentRuntimeReady,
            remoteService: remoteService,
            realtimeService: realtimeService,
            commandService: commandService,
            turnProcessService: turnProcessService,
            messageTaskGraphService: messageTaskGraphService,
            runtimeSettingsService: runtimeSettingsService,
            askUserPromptService: askUserPromptService
        )
    }
}
