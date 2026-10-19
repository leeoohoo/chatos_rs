import ChatOSCore
import ChatOSConnector
import Combine
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class ConversationSessionViewModelTests: XCTestCase {
    func testAttachmentLoaderRejectsOversizedFileBeforeReadingContents() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("conversation-oversized-\(UUID().uuidString).bin")
        defer { try? FileManager.default.removeItem(at: url) }
        XCTAssertTrue(FileManager.default.createFile(atPath: url.path, contents: nil))
        let handle = try FileHandle(forWritingTo: url)
        try handle.truncate(atOffset: UInt64(20 * 1_024 * 1_024 + 1))
        try handle.close()

        let result = ConversationSessionViewModel.loadAttachmentFiles([url])

        XCTAssertTrue(result.attachments.isEmpty)
        XCTAssertEqual(result.errors.count, 1)
    }

    func testAttachmentLoaderBoundsAggregateBytesBeforeRetainingAllFiles() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("conversation-aggregate-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let first = directory.appendingPathComponent("first.bin")
        let second = directory.appendingPathComponent("second.bin")
        for url in [first, second] {
            XCTAssertTrue(FileManager.default.createFile(atPath: url.path, contents: nil))
            let handle = try FileHandle(forWritingTo: url)
            try handle.truncate(atOffset: UInt64(12 * 1_024 * 1_024))
            try handle.close()
        }

        let result = ConversationSessionViewModel.loadAttachmentFiles([first, second])

        XCTAssertEqual(result.attachments.count, 1)
        XCTAssertLessThanOrEqual(
            result.attachments.reduce(0) { $0 + $1.size },
            ConversationSessionViewModel.maximumAttachmentBytes
        )
        XCTAssertEqual(result.errors, ["附件总大小不能超过 20 MB"])
    }

    func testAttachmentLoaderInspectsAtMostMaximumAttachmentCount() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("conversation-count-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let urls = try (0..<25).map { index in
            let url = directory.appendingPathComponent("\(index).txt")
            try Data("x".utf8).write(to: url)
            return url
        }

        let result = ConversationSessionViewModel.loadAttachmentFiles(urls)

        XCTAssertEqual(result.attachments.count, ConversationSessionViewModel.maximumAttachmentCount)
        XCTAssertEqual(result.errors, ["单次最多添加 20 个附件"])
    }

    func testTimelineObservationIgnoresUnrelatedConversationUpdates() async throws {
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore()
        )
        try await Task.sleep(for: .milliseconds(50))
        var timelineUpdateCount = 0
        let timelineCancellable = viewModel.timelineObservationState.objectWillChange.sink {
            timelineUpdateCount += 1
        }
        let initialRevision = viewModel.timelineObservationState.revision

        viewModel.isSending = true
        viewModel.sendError = "send failed"
        viewModel.historyError = "history failed"
        viewModel.runtimeSettingsError = "settings failed"

        XCTAssertEqual(timelineUpdateCount, 0)
        XCTAssertEqual(viewModel.timelineObservationState.revision, initialRevision)

        viewModel.focus(turnID: "turn-1", promptID: nil, taskID: nil, runID: nil)

        XCTAssertEqual(timelineUpdateCount, 1)
        XCTAssertEqual(viewModel.timelineObservationState.revision, initialRevision + 1)
        withExtendedLifetime(timelineCancellable) {}
    }

    func testComposerDraftPublishesOnlyComposerStateUpdates() async throws {
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore()
        )
        try await Task.sleep(for: .milliseconds(50))
        var conversationUpdateCount = 0
        var composerUpdateCount = 0
        let conversationCancellable = viewModel.objectWillChange.sink {
            conversationUpdateCount += 1
        }
        let composerCancellable = viewModel.composerState.objectWillChange.sink {
            composerUpdateCount += 1
        }
        let initialTimelineBuildCount = viewModel.timelineItemsBuildCount

        for index in 1 ... 200 {
            viewModel.draft = String(repeating: "x", count: index)
        }

        XCTAssertEqual(viewModel.composerState.draft.count, 200)
        XCTAssertEqual(conversationUpdateCount, 0)
        XCTAssertEqual(composerUpdateCount, 200)
        XCTAssertEqual(viewModel.timelineItemsBuildCount, initialTimelineBuildCount)
        withExtendedLifetime((conversationCancellable, composerCancellable)) {}
    }

    func testComposerAttachmentStateDoesNotPublishConversationUpdates() async throws {
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore()
        )
        try await Task.sleep(for: .milliseconds(50))
        var conversationUpdateCount = 0
        var composerUpdateCount = 0
        let conversationCancellable = viewModel.objectWillChange.sink {
            conversationUpdateCount += 1
        }
        let composerCancellable = viewModel.composerState.objectWillChange.sink {
            composerUpdateCount += 1
        }

        viewModel.addLongPastedText("attachment content")
        viewModel.attachmentError = "failed"

        XCTAssertEqual(viewModel.composerState.attachments.count, 1)
        XCTAssertEqual(viewModel.composerState.attachmentError, "failed")
        XCTAssertEqual(conversationUpdateCount, 0)
        XCTAssertEqual(composerUpdateCount, 3)
        withExtendedLifetime((conversationCancellable, composerCancellable)) {}
    }

    func testComposerCapabilitySelectionIsForwardedAndClearedAfterSend() async throws {
        let commandService = ConversationCommandServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            commandService: commandService
        )
        viewModel.draft = "帮我整理成一份正式文档"
        viewModel.selectedCapability = .plugin(
            key: "document-plugin",
            componentKey: "document-mcp",
            displayName: "Document"
        )

        viewModel.sendDraft()

        try await waitUntil {
            await commandService.commands().count == 1 && !viewModel.isSending
        }
        let commands = await commandService.commands()
        let command = try XCTUnwrap(commands.first)
        XCTAssertEqual(
            command.explicitCapabilitySelection,
            .plugin(key: "document-plugin", componentKey: "document-mcp", displayName: "Document")
        )
        XCTAssertNil(viewModel.selectedCapability)
    }

    func testRuntimeUnavailableRecoversSendAndRejectsLateHostResponse() async throws {
        let commandService = ConversationCommandServiceStub(delayNewTurn: true)
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            commandService: commandService
        )
        viewModel.draft = "需要重新发送的内容"

        viewModel.sendDraft()
        try await waitUntil {
            let hasDelayedNewTurn = await commandService.hasDelayedNewTurn()
            return hasDelayedNewTurn && viewModel.turns.count == 1
        }
        viewModel.localAgentRuntimeDidBecomeUnavailable()
        try await waitUntil {
            !viewModel.isSending && viewModel.turns.isEmpty
        }

        XCTAssertEqual(viewModel.draft, "需要重新发送的内容")
        XCTAssertEqual(viewModel.sendError, "本地任务组件连接已中断，请重新发送。")
        viewModel.draft = "用户后来输入的内容"

        await commandService.resumeDelayedNewTurn()
        try await Task.sleep(for: .milliseconds(30))

        XCTAssertEqual(viewModel.draft, "用户后来输入的内容")
        XCTAssertTrue(viewModel.turns.isEmpty)
        XCTAssertFalse(viewModel.isSending)
    }

    func testInactiveGuidanceFallsBackToNewTurnWithinSameSendLifecycle() async throws {
        let commandService = ConversationCommandServiceStub(
            rejectGuidanceAsInactive: true
        )
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [ConversationRemoteServiceStub.turn(revision: 1)],
            historyStore: ConversationHistoryStore(),
            commandService: commandService
        )
        viewModel.draft = "改用新消息继续"

        viewModel.sendDraft()
        try await waitUntil {
            await commandService.commands().count == 2 && !viewModel.isSending
        }

        let commands = await commandService.commands()
        XCTAssertEqual(commands.first?.turnID, "turn-1")
        XCTAssertNotEqual(commands.last?.turnID, "turn-1")
        XCTAssertEqual(commands.last?.content, "改用新消息继续")
        XCTAssertNil(viewModel.sendLifecycleState.activeToken)
    }

    func testReconnectReconcileRefreshesConversationHistory() async throws {
        let remoteService = ConversationRemoteServiceStub()
        let realtimeService = ConversationRealtimeServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            remoteService: remoteService,
            realtimeService: realtimeService
        )
        viewModel.activate()

        try await waitUntil {
            let hasSubscriber = await realtimeService.hasSubscriber(sessionID: "session-1")
            return viewModel.turns.first?.revision == 1
                && !viewModel.isRefreshing
                && hasSubscriber
        }

        await realtimeService.yield(
            ConversationRealtimeSignal(
                eventID: "reconcile-1",
                eventSequence: 0,
                sessionID: "session-1",
                turnID: nil,
                kind: .reconcile,
                eventName: "conversation.reconcile",
                timestamp: "2026-09-03T08:00:00Z"
            )
        )

        try await waitUntil {
            viewModel.turns.first?.revision == 2
                && viewModel.turns.first?.finalAssistantMessage?.text == "任务已完成"
        }

        let requestedSessionIDs = await remoteService.requestedSessionIDs()
        XCTAssertEqual(requestedSessionIDs, ["session-1", "session-1"])
    }

    func testRealtimeReconcileRefreshesSilently() async throws {
        let remoteService = ConversationRemoteServiceStub()
        let realtimeService = ConversationRealtimeServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            remoteService: remoteService,
            realtimeService: realtimeService
        )
        viewModel.activate()

        try await waitUntil {
            await remoteService.requestCount() == 1 && !viewModel.isRefreshing
        }
        await remoteService.setFetchDelay(milliseconds: 300)

        await realtimeService.yield(Self.reconcileSignal(id: "reconcile-silent"))

        try await waitUntil {
            await remoteService.requestCount() == 2
        }
        XCTAssertFalse(viewModel.isRefreshing)
    }

    func testRealtimeSignalsAreCoalescedIntoOneRefresh() async throws {
        let remoteService = ConversationRemoteServiceStub()
        let realtimeService = ConversationRealtimeServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            remoteService: remoteService,
            realtimeService: realtimeService
        )
        viewModel.activate()

        try await waitUntil {
            let hasSubscriber = await realtimeService.hasSubscriber(sessionID: "session-1")
            return await remoteService.requestCount() == 1
                && !viewModel.isRefreshing
                && hasSubscriber
        }

        await realtimeService.yield(Self.reconcileSignal(id: "reconcile-1"))
        await realtimeService.yield(Self.reconcileSignal(id: "reconcile-2"))
        await realtimeService.yield(Self.reconcileSignal(id: "reconcile-3"))

        try await waitUntil {
            await remoteService.requestCount() == 2
        }
        try await Task.sleep(for: .milliseconds(350))
        let requestCount = await remoteService.requestCount()
        XCTAssertEqual(requestCount, 2)
    }

    func testUnchangedSnapshotDoesNotPublishViewUpdates() async throws {
        let turn = ConversationRemoteServiceStub.turn(revision: 1)
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [turn],
            historyStore: ConversationHistoryStore()
        )

        try await waitUntil { viewModel.turns == [turn] }
        try await Task.sleep(for: .milliseconds(50))
        var updateCount = 0
        let cancellable = viewModel.objectWillChange.sink {
            updateCount += 1
        }

        await viewModel.refreshSnapshot()

        XCTAssertEqual(updateCount, 0)
        withExtendedLifetime(cancellable) {}
    }

    func testTaskGraphAvailabilityCachesAreRemovedWithDiscardedTurns() async throws {
        let historyStore = ConversationHistoryStore()
        let optimistic = ConversationRemoteServiceStub.turn(revision: 0)
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [optimistic],
            historyStore: historyStore
        )

        try await waitUntil {
            viewModel.taskGraphAvailabilityCacheEntryCount == 1
        }
        await historyStore.discardOptimisticTurn(
            sessionID: "session-1",
            turnID: optimistic.id
        )
        await viewModel.refreshSnapshot()

        XCTAssertTrue(viewModel.turns.isEmpty)
        XCTAssertEqual(viewModel.taskGraphAvailabilityCacheEntryCount, 0)
    }

    func testRuntimeUnavailableRejectsLateTaskGraphAvailabilityAndReadyRetries() async throws {
        var turn = ConversationRemoteServiceStub.turn(revision: 1)
        turn.messageTaskLookup = MessageTaskLookup(
            sessionID: turn.sessionID,
            turnID: turn.id,
            sourceUserMessageID: turn.userMessage.id
        )
        let graphService = DelayedMessageTaskGraphServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [turn],
            historyStore: ConversationHistoryStore(),
            messageTaskGraphService: graphService
        )
        try await waitUntil {
            await graphService.requestCount() == 1
        }

        viewModel.localAgentRuntimeDidBecomeUnavailable()
        await graphService.resumeNextRequest()
        try await Task.sleep(for: .milliseconds(30))

        XCTAssertFalse(viewModel.hasTaskGraph(for: turn))
        XCTAssertTrue(viewModel.taskGraphAvailabilityState.activeTokens.isEmpty)

        viewModel.localAgentRuntimeDidBecomeReady()
        try await waitUntil {
            await graphService.requestCount() == 2
        }
        await graphService.resumeNextRequest()
        try await waitUntil {
            viewModel.hasTaskGraph(for: turn)
        }

        XCTAssertTrue(viewModel.taskGraphAvailabilityState.activeTokens.isEmpty)
    }

    func testDeactivateStopsRealtimeSubscription() async throws {
        let realtimeService = ConversationRealtimeServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            realtimeService: realtimeService
        )
        viewModel.activate()
        try await waitUntil {
            await realtimeService.hasSubscriber(sessionID: "session-1")
        }

        viewModel.deactivate()

        try await waitUntil {
            await realtimeService.wasTerminated(sessionID: "session-1")
        }
    }

    func testDeactivateRejectsLateHistoryRefreshPresentation() async throws {
        let remoteService = ConversationRemoteServiceStub()
        await remoteService.setFetchDelay(milliseconds: 300)
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            remoteService: remoteService
        )
        viewModel.activate()
        try await waitUntil {
            await remoteService.requestCount() == 1
        }

        viewModel.deactivate()
        try await Task.sleep(for: .milliseconds(350))

        XCTAssertTrue(viewModel.turns.isEmpty)
        XCTAssertNil(viewModel.historyError)
        XCTAssertFalse(viewModel.isRefreshing)
    }

    func testDeactivateRejectsLateOlderHistoryPage() async throws {
        let remoteService = DelayedOlderConversationRemoteServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            remoteService: remoteService
        )
        viewModel.activate()
        try await waitUntil {
            viewModel.hasOlder && viewModel.turns.map(\.id) == ["turn-current"]
        }
        viewModel.loadOlder()
        try await waitUntil {
            await remoteService.hasDelayedOlderRequest()
        }

        viewModel.deactivate()
        await remoteService.resumeOlderRequest()
        try await Task.sleep(for: .milliseconds(30))

        XCTAssertEqual(viewModel.turns.map(\.id), ["turn-current"])
        XCTAssertFalse(viewModel.isLoadingOlder)
        XCTAssertNil(viewModel.historyError)
    }

    func testRuntimeBecomingReadyReloadsModelsAfterInitialBootstrapFailure() async throws {
        let runtimeSettings = RecoveringRuntimeSettingsServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            localAgentRuntimeReady: false,
            runtimeSettingsService: runtimeSettings
        )

        try await Task.sleep(for: .milliseconds(50))
        XCTAssertNil(viewModel.runtimeSettingsError)
        XCTAssertTrue(viewModel.availableModels.isEmpty)

        viewModel.localAgentRuntimeDidBecomeReady()
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertNil(viewModel.runtimeSettingsError)

        await runtimeSettings.configure()
        viewModel.localAgentRuntimeDidBecomeReady()

        try await waitUntil {
            viewModel.runtimeSettingsError == nil
                && viewModel.availableModels.map(\.id) == ["model-1"]
                && viewModel.selectedModelID == "model-1"
        }
    }

    func testRuntimeUnavailableRejectsLateReadyRecovery() async throws {
        let remoteService = ConversationRemoteServiceStub()
        let runtimeSettings = DelayedRuntimeSettingsServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            localAgentRuntimeReady: false,
            remoteService: remoteService,
            runtimeSettingsService: runtimeSettings
        )
        viewModel.activate()
        viewModel.localAgentRuntimeDidBecomeReady()
        try await waitUntil {
            await runtimeSettings.pendingRequestCount() >= 2
        }

        viewModel.localAgentRuntimeDidBecomeUnavailable()
        await runtimeSettings.resumeRequests()
        try await Task.sleep(for: .milliseconds(300))

        let requestCount = await remoteService.requestCount()
        XCTAssertFalse(viewModel.isLocalAgentRuntimeReady)
        XCTAssertEqual(requestCount, 0)
    }

    func testNewReadyRecoveryRejectsOlderLateCompletion() async throws {
        let remoteService = ConversationRemoteServiceStub()
        let runtimeSettings = DelayedRuntimeSettingsServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            localAgentRuntimeReady: false,
            remoteService: remoteService,
            runtimeSettingsService: runtimeSettings
        )
        try await Task.sleep(for: .milliseconds(50))
        viewModel.activate()

        viewModel.localAgentRuntimeDidBecomeReady()
        try await waitUntil {
            await runtimeSettings.pendingRequestCount() >= 2
        }
        viewModel.localAgentRuntimeDidBecomeReady()
        try await waitUntil {
            await runtimeSettings.pendingRequestCount() >= 4
        }

        await runtimeSettings.resumeRequests()
        try await waitUntil {
            await remoteService.requestCount() == 1
        }
        try await Task.sleep(for: .milliseconds(50))

        let requestCount = await remoteService.requestCount()
        XCTAssertEqual(requestCount, 1)
        XCTAssertNil(viewModel.runtimeRecoveryState.activeToken)
    }

    func testNewRuntimeSettingsMutationRejectsOlderModelResponse() async throws {
        let runtimeSettings = RacingRuntimeSettingsServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            runtimeSettingsService: runtimeSettings
        )
        try await waitUntil {
            viewModel.availableModels.count == 3
                && viewModel.selectedModelID == "model-1"
        }

        viewModel.setSelectedModelID("model-2")
        try await waitUntil {
            await runtimeSettings.hasDelayedModelUpdate()
        }
        viewModel.setSelectedModelID("model-3")
        try await waitUntil {
            viewModel.selectedModelID == "model-3"
                && !viewModel.isUpdatingRuntimeSettings
        }
        await runtimeSettings.resumeDelayedModelUpdate()
        try await Task.sleep(for: .milliseconds(30))

        XCTAssertEqual(viewModel.selectedModelID, "model-3")
        XCTAssertNil(viewModel.runtimeSettingsError)
        XCTAssertFalse(viewModel.isUpdatingRuntimeSettings)
    }

    func testStalePromptRefreshCannotOverwriteSubmittedPrompt() async throws {
        let promptService = DelayedAskUserPromptServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            askUserPromptService: promptService
        )

        try await waitUntil {
            viewModel.askUserPrompts.first?.status == .pending
        }
        let prompt = try XCTUnwrap(viewModel.askUserPrompts.first)
        let refreshTask = Task { await viewModel.refreshAskUserPrompts() }
        try await waitUntil {
            await promptService.hasDelayedFetch()
        }

        viewModel.submitAskUserPrompt(prompt, submission: .init())
        try await waitUntil {
            viewModel.askUserPrompts.first?.status == .ok
        }
        await promptService.resumeDelayedFetch()
        await refreshTask.value

        XCTAssertEqual(viewModel.askUserPrompts.first?.status, .ok)
    }

    func testAskUserSubmitIsExclusivePerPrompt() async throws {
        let promptService = DelayedAskUserPromptServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            askUserPromptService: promptService
        )
        try await waitUntil {
            viewModel.askUserPrompts.first?.status == .pending
        }
        let prompt = try XCTUnwrap(viewModel.askUserPrompts.first)
        await promptService.delayNextSubmit()

        viewModel.submitAskUserPrompt(prompt, submission: .init())
        viewModel.submitAskUserPrompt(prompt, submission: .init())
        try await waitUntil { await promptService.hasDelayedSubmit() }

        XCTAssertTrue(viewModel.isSubmitting(promptID: prompt.id))
        let submitCallCount = await promptService.submitCallCount()
        XCTAssertEqual(submitCallCount, 1)
        await promptService.resumeDelayedSubmit()
        try await waitUntil { !viewModel.isSubmitting(promptID: prompt.id) }
        XCTAssertEqual(viewModel.askUserPrompts.first?.status, .ok)
    }

    func testRuntimeUnavailableRejectsLateAskUserAction() async throws {
        let promptService = DelayedAskUserPromptServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            askUserPromptService: promptService
        )
        try await waitUntil {
            viewModel.askUserPrompts.first?.status == .pending
        }
        let prompt = try XCTUnwrap(viewModel.askUserPrompts.first)
        await promptService.delayNextSubmit()
        viewModel.submitAskUserPrompt(prompt, submission: .init())
        try await waitUntil { await promptService.hasDelayedSubmit() }

        viewModel.localAgentRuntimeDidBecomeUnavailable()
        XCTAssertFalse(viewModel.isSubmitting(promptID: prompt.id))
        XCTAssertTrue(viewModel.askUserPromptErrors.isEmpty)
        await promptService.resumeDelayedSubmit()
        try await Task.sleep(for: .milliseconds(30))

        XCTAssertEqual(viewModel.askUserPrompts.first?.status, .pending)
        XCTAssertFalse(viewModel.isSubmitting(promptID: prompt.id))
        XCTAssertTrue(viewModel.askUserPromptErrors.isEmpty)
    }

    func testPromptFocusWaitsForPromptRefreshBeforeNavigating() async throws {
        let promptService = DelayedAskUserPromptServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            askUserPromptService: promptService
        )
        try await waitUntil {
            viewModel.askUserPrompts.first?.status == .pending
        }

        let focusTask = Task {
            await viewModel.focusAfterRefreshingAskUserPrompts(
                turnID: "turn-1",
                promptID: "prompt-1",
                taskID: "task-1",
                runID: "run-1"
            )
        }
        try await waitUntil {
            await promptService.hasDelayedFetch()
        }
        XCTAssertNil(viewModel.focusRequest)

        await promptService.resumeDelayedFetch()
        await focusTask.value

        XCTAssertEqual(viewModel.focusRequest?.turnID, "turn-1")
        XCTAssertEqual(viewModel.focusRequest?.promptID, "prompt-1")
        XCTAssertEqual(viewModel.focusRequest?.taskID, "task-1")
        XCTAssertEqual(viewModel.focusRequest?.runID, "run-1")
    }

    func testActivateRefreshesAskUserPromptsMissedWhileConversationWasInactive() async throws {
        let promptService = DelayedAskUserPromptServiceStub()
        let viewModel = ConversationSessionViewModel(
            sessionID: "session-1",
            initialTurns: [],
            historyStore: ConversationHistoryStore(),
            askUserPromptService: promptService
        )
        try await waitUntil {
            await promptService.fetchCallCount() == 1
        }

        viewModel.activate()

        try await waitUntil {
            await promptService.hasDelayedFetch()
        }
        let fetchCallCount = await promptService.fetchCallCount()
        XCTAssertEqual(fetchCallCount, 2)
        await promptService.resumeDelayedFetch()
        try await waitUntil {
            viewModel.askUserPrompts.first?.status == .pending
        }
    }

    private static func reconcileSignal(id: String) -> ConversationRealtimeSignal {
        ConversationRealtimeSignal(
            eventID: id,
            eventSequence: 0,
            sessionID: "session-1",
            turnID: nil,
            kind: .reconcile,
            eventName: "conversation.reconcile",
            timestamp: "2026-09-03T08:00:00Z"
        )
    }

    private func waitUntil(
        timeoutIterations: Int = 100,
        condition: @escaping @MainActor () async -> Bool
    ) async throws {
        for _ in 0..<timeoutIterations {
            if await condition() { return }
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTFail("Timed out waiting for asynchronous conversation state")
    }
}

private actor DelayedMessageTaskGraphServiceStub: MessageTaskGraphServicing {
    private enum StubError: Error {
        case unexpectedCall
    }

    private var requestContinuations: [CheckedContinuation<Void, Never>] = []
    private var graphRequestCount = 0

    func requestCount() -> Int { graphRequestCount }

    func resumeNextRequest() {
        guard !requestContinuations.isEmpty else { return }
        requestContinuations.removeFirst().resume()
    }

    func fetchGraph(
        messageID _: String,
        lookup _: MessageTaskLookup?
    ) async -> MessageTaskGraphSnapshot {
        graphRequestCount += 1
        await withCheckedContinuation { continuation in
            requestContinuations.append(continuation)
        }
        let task = MessageTask(id: "task-1", title: "Task")
        return MessageTaskGraphSnapshot(
            rootTaskIDs: [task.id],
            nodes: [MessageTaskGraphNode(
                task: task,
                depth: 0,
                isRoot: true,
                isCurrentMessage: true
            )],
            edges: []
        )
    }

    func fetchTask(
        messageID _: String,
        taskID _: String,
        lookup _: MessageTaskLookup?
    ) async throws -> MessageTask {
        throw StubError.unexpectedCall
    }

    func fetchRun(
        messageID _: String,
        runID _: String,
        lookup _: MessageTaskLookup?,
        includeEvents _: Bool,
        eventLimit _: Int,
        eventOffset _: Int
    ) async throws -> MessageTaskRunDetail {
        throw StubError.unexpectedCall
    }

    func retryRun(
        messageID _: String,
        runID _: String,
        lookup _: MessageTaskLookup?,
        instruction _: String?
    ) async throws -> MessageTaskRun {
        throw StubError.unexpectedCall
    }

    func cancelTask(
        messageID _: String,
        taskID _: String,
        lookup _: MessageTaskLookup?,
        reason _: String?
    ) async throws {
        throw StubError.unexpectedCall
    }
}

private actor ConversationCommandServiceStub: ConversationCommandServicing {
    private let delayNewTurn: Bool
    private let rejectGuidanceAsInactive: Bool
    private var recordedCommands: [ConversationSendCommand] = []
    private var delayedNewTurnContinuation: CheckedContinuation<Void, Never>?

    init(
        delayNewTurn: Bool = false,
        rejectGuidanceAsInactive: Bool = false
    ) {
        self.delayNewTurn = delayNewTurn
        self.rejectGuidanceAsInactive = rejectGuidanceAsInactive
    }

    func sendNewTurn(_ command: ConversationSendCommand) async throws -> ConversationCommandAck {
        recordedCommands.append(command)
        if delayNewTurn {
            await withCheckedContinuation { continuation in
                delayedNewTurnContinuation = continuation
            }
        }
        return ConversationCommandAck(
            accepted: true,
            turnID: command.turnID,
            userMessageID: "message-1"
        )
    }

    func sendGuidance(_ command: ConversationSendCommand) async throws -> ConversationCommandAck {
        recordedCommands.append(command)
        if rejectGuidanceAsInactive {
            throw ConversationCommandError.guidanceTargetInactive
        }
        return ConversationCommandAck(
            accepted: true,
            turnID: command.turnID,
            userMessageID: "message-1"
        )
    }

    func stopTurn(conversationID: String, turnID: String?) async throws {}

    func commands() -> [ConversationSendCommand] { recordedCommands }

    func hasDelayedNewTurn() -> Bool {
        delayedNewTurnContinuation != nil
    }

    func resumeDelayedNewTurn() {
        delayedNewTurnContinuation?.resume()
        delayedNewTurnContinuation = nil
    }
}

private actor DelayedAskUserPromptServiceStub: AskUserPromptServicing {
    private let pendingPrompt = AskUserPrompt(
        id: "prompt-1",
        sessionID: "session-1",
        turnID: "turn-1",
        kind: "choice",
        status: .pending,
        title: "Choose",
        message: "Pick one",
        allowsCancel: true
    )
    private var fetchCount = 0
    private var delayedFetchContinuation: CheckedContinuation<Void, Never>?
    private var shouldDelayNextSubmit = false
    private var delayedSubmitContinuation: CheckedContinuation<Void, Never>?
    private var submitCount = 0

    func fetchPrompts(sessionID: String, limit: Int) async throws -> [AskUserPrompt] {
        fetchCount += 1
        if fetchCount > 1 {
            await withCheckedContinuation { continuation in
                delayedFetchContinuation = continuation
            }
        }
        return [pendingPrompt]
    }

    func submit(
        promptID: String,
        sessionID: String,
        submission: AskUserSubmission
    ) async throws -> AskUserPrompt {
        submitCount += 1
        if shouldDelayNextSubmit {
            shouldDelayNextSubmit = false
            await withCheckedContinuation { continuation in
                delayedSubmitContinuation = continuation
            }
        }
        var submitted = pendingPrompt
        submitted.status = .ok
        return submitted
    }

    func cancel(promptID: String, sessionID: String) async throws -> AskUserPrompt {
        var canceled = pendingPrompt
        canceled.status = .canceled
        return canceled
    }

    func hasDelayedFetch() -> Bool { delayedFetchContinuation != nil }

    func fetchCallCount() -> Int { fetchCount }

    func delayNextSubmit() {
        shouldDelayNextSubmit = true
    }

    func hasDelayedSubmit() -> Bool { delayedSubmitContinuation != nil }

    func submitCallCount() -> Int { submitCount }

    func resumeDelayedFetch() {
        delayedFetchContinuation?.resume()
        delayedFetchContinuation = nil
    }

    func resumeDelayedSubmit() {
        delayedSubmitContinuation?.resume()
        delayedSubmitContinuation = nil
    }
}

private actor ConversationRemoteServiceStub: ConversationRemoteServicing {
    private var queries: [ConversationHistoryQuery] = []
    private var fetchDelayMilliseconds = 0

    func fetchHistory(_ query: ConversationHistoryQuery) async throws -> HistoryPage {
        queries.append(query)
        let revision = Int64(queries.count)
        let delay = fetchDelayMilliseconds
        if delay > 0 {
            try await Task.sleep(for: .milliseconds(delay))
        }
        return HistoryPage(
            turns: [Self.turn(revision: revision)],
            olderCursor: nil,
            hasOlder: false,
            snapshotRevision: revision,
            requestGeneration: query.requestGeneration
        )
    }

    func issueWebSocketTicket() async throws -> String {
        "ticket"
    }

    func requestedSessionIDs() -> [String] {
        queries.map(\.sessionID)
    }

    func requestCount() -> Int {
        queries.count
    }

    func setFetchDelay(milliseconds: Int) {
        fetchDelayMilliseconds = milliseconds
    }

    static func turn(revision: Int64) -> ConversationTurn {
        ConversationTurn(
            id: "turn-1",
            sessionID: "session-1",
            sequence: 1,
            revision: revision,
            userMessage: ChatMessage(
                id: "message-1",
                role: .user,
                text: "执行任务",
                createdAt: Date(timeIntervalSince1970: 1)
            ),
            finalAssistantMessage: revision > 1
                ? ChatMessage(
                    id: "assistant-1",
                    role: .assistant,
                    text: "任务已完成",
                    createdAt: Date(timeIntervalSince1970: 2)
                )
                : nil,
            status: revision > 1 ? .completed : .streaming,
            startedAt: Date(timeIntervalSince1970: 1),
            completedAt: revision > 1 ? Date(timeIntervalSince1970: 2) : nil
        )
    }
}

private actor DelayedOlderConversationRemoteServiceStub: ConversationRemoteServicing {
    private var olderContinuation: CheckedContinuation<Void, Never>?

    func fetchHistory(_ query: ConversationHistoryQuery) async -> HistoryPage {
        if query.before != nil {
            await withCheckedContinuation { continuation in
                olderContinuation = continuation
            }
            return HistoryPage(
                turns: [Self.turn(id: "turn-older", sequence: 1)],
                olderCursor: nil,
                hasOlder: false,
                snapshotRevision: 2,
                requestGeneration: query.requestGeneration
            )
        }
        return HistoryPage(
            turns: [Self.turn(id: "turn-current", sequence: 2)],
            olderCursor: "older-cursor",
            hasOlder: true,
            snapshotRevision: 1,
            requestGeneration: query.requestGeneration
        )
    }

    func issueWebSocketTicket() async -> String { "ticket" }

    func hasDelayedOlderRequest() -> Bool { olderContinuation != nil }

    func resumeOlderRequest() {
        olderContinuation?.resume()
        olderContinuation = nil
    }

    private static func turn(id: String, sequence: Int64) -> ConversationTurn {
        ConversationTurn(
            id: id,
            sessionID: "session-1",
            sequence: sequence,
            revision: 1,
            userMessage: ChatMessage(
                id: "message-\(id)",
                role: .user,
                text: id,
                createdAt: Date(timeIntervalSince1970: TimeInterval(sequence))
            ),
            status: .completed,
            startedAt: Date(timeIntervalSince1970: TimeInterval(sequence))
        )
    }
}

private actor ConversationRealtimeServiceStub: ConversationRealtimeStreaming {
    private var continuations: [
        String: AsyncThrowingStream<ConversationRealtimeSignal, Error>.Continuation
    ] = [:]

    func events(
        sessionID: String
    ) async -> AsyncThrowingStream<ConversationRealtimeSignal, Error> {
        let (stream, continuation) = AsyncThrowingStream.makeStream(
            of: ConversationRealtimeSignal.self,
            throwing: Error.self
        )
        continuations[sessionID] = continuation
        continuation.onTermination = { [weak self] _ in
            Task { await self?.markTerminated(sessionID: sessionID) }
        }
        return stream
    }

    func hasSubscriber(sessionID: String) -> Bool {
        continuations[sessionID] != nil
    }

    func yield(_ signal: ConversationRealtimeSignal) {
        continuations[signal.sessionID]?.yield(signal)
    }

    private var terminatedSessionIDs: Set<String> = []

    func wasTerminated(sessionID: String) -> Bool {
        terminatedSessionIDs.contains(sessionID)
    }

    private func markTerminated(sessionID: String) {
        continuations.removeValue(forKey: sessionID)
        terminatedSessionIDs.insert(sessionID)
    }
}

private actor RecoveringRuntimeSettingsServiceStub: ConversationRuntimeSettingsServicing {
    private var isConfigured = false

    func configure() {
        isConfigured = true
    }

    func fetchSettings(sessionID _: String) throws -> ConversationRuntimeSettings {
        try requireConfigured()
        return ConversationRuntimeSettings(
            selectedModelID: "model-1",
            selectedModelName: "Local model",
            selectedThinkingLevel: "medium",
            remoteConnectionID: nil,
            reasoningEnabled: true
        )
    }

    func fetchAvailableModels() throws -> [ConversationModelOption] {
        try requireConfigured()
        return [ConversationModelOption(
            id: "model-1",
            displayName: "Local model",
            modelName: "local-model",
            provider: "openai",
            thinkingLevel: "medium",
            supportsReasoning: true,
            thinkingLevels: ["none", "medium"]
        )]
    }

    func updateModel(
        sessionID: String,
        modelID _: String
    ) throws -> ConversationRuntimeSettings {
        try fetchSettings(sessionID: sessionID)
    }

    func updateRemoteConnection(
        sessionID: String,
        connectionID _: String?
    ) throws -> ConversationRuntimeSettings {
        try fetchSettings(sessionID: sessionID)
    }

    func updateReasoning(
        sessionID: String,
        enabled _: Bool
    ) throws -> ConversationRuntimeSettings {
        try fetchSettings(sessionID: sessionID)
    }

    func updateReasoningLevel(
        sessionID: String,
        level _: String,
        enabled _: Bool
    ) throws -> ConversationRuntimeSettings {
        try fetchSettings(sessionID: sessionID)
    }

    private func requireConfigured() throws {
        guard isConfigured else {
            throw NativeLocalAgentConversationRuntimeSettingsError.notConfigured
        }
    }
}

private actor DelayedRuntimeSettingsServiceStub: ConversationRuntimeSettingsServicing {
    private var continuations: [CheckedContinuation<Void, Never>] = []
    private var isReleased = false

    func pendingRequestCount() -> Int { continuations.count }

    func resumeRequests() {
        isReleased = true
        let pending = continuations
        continuations.removeAll()
        pending.forEach { $0.resume() }
    }

    func fetchSettings(sessionID _: String) async -> ConversationRuntimeSettings {
        await waitUntilResumed()
        return ConversationRuntimeSettings(selectedModelID: "model-1")
    }

    func fetchAvailableModels() async -> [ConversationModelOption] {
        await waitUntilResumed()
        return [ConversationModelOption(
            id: "model-1",
            displayName: "Local model",
            modelName: "local-model"
        )]
    }

    func updateModel(
        sessionID _: String,
        modelID _: String
    ) async -> ConversationRuntimeSettings {
        ConversationRuntimeSettings(selectedModelID: "model-1")
    }

    func updateRemoteConnection(
        sessionID _: String,
        connectionID _: String?
    ) async -> ConversationRuntimeSettings {
        ConversationRuntimeSettings(selectedModelID: "model-1")
    }

    func updateReasoning(
        sessionID _: String,
        enabled _: Bool
    ) async -> ConversationRuntimeSettings {
        ConversationRuntimeSettings(selectedModelID: "model-1")
    }

    func updateReasoningLevel(
        sessionID _: String,
        level _: String,
        enabled _: Bool
    ) async -> ConversationRuntimeSettings {
        ConversationRuntimeSettings(selectedModelID: "model-1")
    }

    private func waitUntilResumed() async {
        guard !isReleased else { return }
        await withCheckedContinuation { continuation in
            continuations.append(continuation)
        }
    }
}

private actor RacingRuntimeSettingsServiceStub: ConversationRuntimeSettingsServicing {
    private var delayedModelContinuation: CheckedContinuation<Void, Never>?

    func hasDelayedModelUpdate() -> Bool { delayedModelContinuation != nil }

    func resumeDelayedModelUpdate() {
        delayedModelContinuation?.resume()
        delayedModelContinuation = nil
    }

    func fetchSettings(sessionID _: String) -> ConversationRuntimeSettings {
        ConversationRuntimeSettings(selectedModelID: "model-1")
    }

    func fetchAvailableModels() -> [ConversationModelOption] {
        (1 ... 3).map { index in
            ConversationModelOption(
                id: "model-\(index)",
                displayName: "Model \(index)",
                modelName: "model-\(index)"
            )
        }
    }

    func updateModel(
        sessionID _: String,
        modelID: String
    ) async -> ConversationRuntimeSettings {
        if modelID == "model-2" {
            await withCheckedContinuation { continuation in
                delayedModelContinuation = continuation
            }
        }
        return ConversationRuntimeSettings(selectedModelID: modelID)
    }

    func updateRemoteConnection(
        sessionID _: String,
        connectionID: String?
    ) -> ConversationRuntimeSettings {
        ConversationRuntimeSettings(
            selectedModelID: "model-1",
            remoteConnectionID: connectionID
        )
    }

    func updateReasoning(
        sessionID _: String,
        enabled: Bool
    ) -> ConversationRuntimeSettings {
        ConversationRuntimeSettings(
            selectedModelID: "model-1",
            reasoningEnabled: enabled
        )
    }

    func updateReasoningLevel(
        sessionID _: String,
        level: String,
        enabled: Bool
    ) -> ConversationRuntimeSettings {
        ConversationRuntimeSettings(
            selectedModelID: "model-1",
            selectedThinkingLevel: level,
            reasoningEnabled: enabled
        )
    }
}
