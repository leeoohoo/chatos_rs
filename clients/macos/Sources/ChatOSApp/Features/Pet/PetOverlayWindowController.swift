@preconcurrency import AppKit
import ChatOSConnector
import ChatOSCore
import Combine
import SwiftUI

@MainActor
final class PetOverlayWindowController: NSWindowController, NSWindowDelegate {

    let store: PetOverlayStore
    private let preferences: PetPreferencesStore
    weak var model: AppModel?
    let messagePanel: NSPanel
    let activityPanel: NSPanel
    let runningActivityPanel: NSPanel
    let fileWorkbenchPanel: NSPanel
    let fileWorkbenchStore: PetFileWorkbenchStore
    let translationViewModel: PetTranslationViewModel
    private let notepadViewModel: NotepadViewModel
    let interactionState = PetOverlayInteractionState()
    let activityInteractionState = PetOverlayInteractionState()
    let runningActivityInteractionState = PetOverlayInteractionState()
    private let onOpen: (PetActivity) -> Void
    private var taskInspectorPanel: NSPanel?
    private var cancellables = Set<AnyCancellable>()
    var isProgrammaticMove = false
    var isDraggingPet = false
    var lastDragOriginX: CGFloat?
    private var isPetRequestedVisible = false
    private var isScreenAwake = true

    init(
        model: AppModel,
        store: PetOverlayStore,
        preferences: PetPreferencesStore,
        approvalViewModel: LocalConnectorControlCenterViewModel,
        onOpen: @escaping (PetActivity) -> Void,
        onRetry: @escaping (PetActivity, String) async throws -> Void,
        onCancel: @escaping (PetActivity) async throws -> Void,
        onLoadTask: @escaping (PetActivity) async throws -> MessageTask,
        onLoadPrompt: @escaping (PetActivity) async throws -> AskUserPrompt,
        onSubmitPrompt: @escaping (AskUserPrompt, AskUserSubmission) async throws -> Void,
        onCancelPrompt: @escaping (AskUserPrompt) async throws -> Void
    ) {
        self.store = store
        self.preferences = preferences
        self.model = model
        self.onOpen = onOpen

        let petPanel = PetOverlayPanelFactory.makePanel(size: NSSize(width: preferences.size, height: preferences.size))
        self.messagePanel = PetOverlayPanelFactory.makePanel(
            size: PetOverlayLayout.compactMessageSize,
            acceptsKeyboardInput: true
        )
        self.activityPanel = PetOverlayPanelFactory.makePanel(
            size: PetOverlayLayout.compactMessageSize,
            acceptsKeyboardInput: true
        )
        self.runningActivityPanel = PetOverlayPanelFactory.makePanel(
            size: PetOverlayLayout.compactMessageSize,
            acceptsKeyboardInput: true
        )
        let fileWorkbenchStore = PetFileWorkbenchStore(service: model.projectFilesystemService)
        self.fileWorkbenchStore = fileWorkbenchStore
        self.translationViewModel = PetTranslationViewModel(
            agent: PetTranslationAgent(services: model.agentServices),
            historyStore: PetTranslationHistoryStore(
                fileURL: RuntimeConfiguration.nativeConnectorStateURL
                    .deletingLastPathComponent()
                    .appendingPathComponent("PetTranslationHistory.json")
            ),
            modelProvider: { [weak model] in
                guard let model else { throw CancellationError() }
                return try await model.localConnectorControl.availableTaskModels()
            }
        )
        self.notepadViewModel = NotepadViewModel(service: model.notepadService)
        self.fileWorkbenchPanel = PetOverlayPanelFactory.makeFileWorkbenchPanel(size: PetOverlayLayout.fileWorkbenchSize)
        super.init(window: petPanel)

        petPanel.title = "ChatOS Pet"
        messagePanel.title = "ChatOS Quick Chat"
        activityPanel.title = "ChatOS Activity"
        runningActivityPanel.title = "ChatOS Running Tasks"
        fileWorkbenchPanel.title = "ChatOS File Workbench"

        petPanel.delegate = self
        petPanel.contentView = PetInteractionHostingView(
            rootView: PetLocalizedRoot(
                model: model,
                content: PetCharacterView(store: store, interactionState: interactionState)
            ),
            onInteractionBegan: { [weak self] in self?.beginMovingPet() },
            onInteractionEnded: { [weak self] didDrag in
                self?.finishPetInteraction(didDrag: didDrag)
            }
        )
        let messageHostingView = NSHostingView(
            rootView: PetLocalizedRoot(
                model: model,
                content: PetQuickChatView(
                    interactionState: interactionState,
                    translationViewModel: translationViewModel,
                    notepadViewModel: notepadViewModel,
                    onInspectTaskReply: { [weak self] selection, service in
                        self?.presentTaskInspector(selection: selection, service: service)
                    }
                )
            )
        )
        messageHostingView.sizingOptions = []
        messageHostingView.frame = NSRect(origin: .zero, size: PetOverlayLayout.compactMessageSize)
        messagePanel.contentView = messageHostingView

        let activityHostingView = NSHostingView(
            rootView: PetLocalizedRoot(
                model: model,
                content: PetMessageView(
                    store: store,
                    interactionState: activityInteractionState,
                    approvalViewModel: approvalViewModel,
                    activityScope: .primary,
                    onOpen: onOpen,
                    onRetry: onRetry,
                    onCancel: onCancel,
                    onLoadTask: onLoadTask,
                    onLoadPrompt: onLoadPrompt,
                    onSubmitPrompt: onSubmitPrompt,
                    onCancelPrompt: onCancelPrompt
                )
            )
        )
        activityHostingView.sizingOptions = []
        activityHostingView.frame = NSRect(origin: .zero, size: PetOverlayLayout.compactMessageSize)
        activityPanel.contentView = activityHostingView
        activityPanel.level = NSWindow.Level(rawValue: messagePanel.level.rawValue + 1)
        applyActivitySize(PetOverlayLayout.compactMessageSize)

        let runningActivityHostingView = NSHostingView(
            rootView: PetLocalizedRoot(
                model: model,
                content: PetMessageView(
                    store: store,
                    interactionState: runningActivityInteractionState,
                    approvalViewModel: approvalViewModel,
                    activityScope: .running,
                    onOpen: onOpen,
                    onRetry: onRetry,
                    onCancel: onCancel,
                    onLoadTask: onLoadTask,
                    onLoadPrompt: onLoadPrompt,
                    onSubmitPrompt: onSubmitPrompt,
                    onCancelPrompt: onCancelPrompt
                )
            )
        )
        runningActivityHostingView.sizingOptions = []
        runningActivityHostingView.frame = NSRect(origin: .zero, size: PetOverlayLayout.compactMessageSize)
        runningActivityPanel.contentView = runningActivityHostingView
        runningActivityPanel.level = NSWindow.Level(rawValue: messagePanel.level.rawValue + 1)
        applyRunningActivitySize(PetOverlayLayout.compactMessageSize)

        let fileWorkbenchHostingView = NSHostingView(
            rootView: PetLocalizedRoot(
                model: model,
                content: PetFileWorkbenchView(
                    store: fileWorkbenchStore,
                    defaultHandlerPrompt: model.petDefaultFileHandlerPrompt
                )
            )
        )
        fileWorkbenchHostingView.sizingOptions = []
        fileWorkbenchHostingView.frame = NSRect(origin: .zero, size: PetOverlayLayout.fileWorkbenchSize)
        fileWorkbenchPanel.contentView = fileWorkbenchHostingView
        (fileWorkbenchPanel as? PetFileWorkbenchPanel)?.onCancel = { [weak fileWorkbenchStore] in
            fileWorkbenchStore?.requestDismiss()
        }

        applyCollectionBehavior()
        restoreOrPlaceDefault()
        bind()
        bindAnimationActivity()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func setVisible(_ visible: Bool) {
        guard let window else { return }
        isPetRequestedVisible = visible
        updateAnimationActivity()
        if visible {
            window.orderFrontRegardless()
            updateMessageVisibility()
            updateFileWorkbenchVisibility()
            updateRunningActivityVisibility()
            updateActivityVisibility()
        } else {
            dismissTaskInspector()
            window.orderOut(nil)
            messagePanel.orderOut(nil)
            fileWorkbenchPanel.orderOut(nil)
            runningActivityPanel.orderOut(nil)
            activityPanel.orderOut(nil)
        }
    }

    private func bindAnimationActivity() {
        translationViewModel.$petAnimationState
            .receive(on: RunLoop.main)
            .sink { [weak self] state in
                self?.interactionState.translationAnimationState = state
            }
            .store(in: &cancellables)

        NSWorkspace.shared.notificationCenter.publisher(
            for: NSWorkspace.screensDidSleepNotification
        )
        .merge(with: NSWorkspace.shared.notificationCenter.publisher(
            for: NSWorkspace.screensDidWakeNotification
        ))
        .receive(on: RunLoop.main)
        .sink { [weak self] notification in
            guard let self else { return }
            isScreenAwake = notification.name == NSWorkspace.screensDidWakeNotification
            updateAnimationActivity()
        }
        .store(in: &cancellables)
    }

    private func updateAnimationActivity() {
        interactionState.isAnimationActive = PetAnimationActivityPolicy.isActive(
            isPetVisible: isPetRequestedVisible,
            isScreenAwake: isScreenAwake
        )
    }

    func openFile(_ request: PetFileOpenRequest) {
        interactionState.isQuickChatPresented = false
        interactionState.isTranslationPresented = false
        interactionState.isNotepadPresented = false
        translationViewModel.cancel()
        dismissTaskInspector()
        fileWorkbenchStore.open(request)
        updateMessageVisibility()
        updateFileWorkbenchVisibility()
        updateRunningActivityVisibility()
        updateActivityVisibility()
    }

    func openTranslationImage(data: Data, suggestedName: String) {
        fileWorkbenchStore.requestDismiss()
        dismissTaskInspector()
        translationViewModel.cancel()
        translationViewModel.selectHistoryRecord(nil)
        translationViewModel.addPastedImage(
            data: data,
            mimeType: "image/png",
            suggestedName: suggestedName
        )
        interactionState.selectedQuickChatResourceID = nil
        interactionState.isNotepadPresented = false
        interactionState.isTranslationPresented = true
        interactionState.isQuickChatPresented = true
        applyQuickChatSize(preferredQuickChatMessageSize())
        updateMessageVisibility()
        updateFileWorkbenchVisibility()
        updateRunningActivityVisibility()
        updateActivityVisibility()
        translationViewModel.translateWhenReady()
    }

    func windowDidMove(_ notification: Notification) {
        guard !isProgrammaticMove else { return }
        if isDraggingPet, let window {
            let currentX = window.frame.origin.x
            if let previousX = lastDragOriginX, abs(currentX - previousX) >= 0.5 {
                interactionState.dragDirection = currentX > previousX ? .right : .left
            }
            lastDragOriginX = currentX
            return
        }
        if messagePanel.isVisible {
            positionMessagePanel()
        }
        if fileWorkbenchPanel.isVisible {
            positionFileWorkbenchPanel()
        }
        if runningActivityPanel.isVisible {
            positionRunningActivityPanel()
        }
        if activityPanel.isVisible {
            positionActivityPanel()
        }
    }

    func windowDidChangeScreen(_ notification: Notification) {
        clampToVisibleScreen()
    }


    private func bind() {
        store.$presentation
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self else { return }
                if self.activityInteractionState.isMessageExpanded {
                    self.applyActivitySize(self.preferredExpandedMessageSize(scope: .primary))
                }
                if self.runningActivityInteractionState.isMessageExpanded {
                    self.applyRunningActivitySize(
                        self.preferredExpandedMessageSize(scope: .running)
                    )
                }
                self.updateRunningActivityVisibility()
                self.updateActivityVisibility()
            }
            .store(in: &cancellables)

        activityInteractionState.$isMessageExpanded
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] expanded in
                guard let self else { return }
                self.applyActivitySize(
                    expanded
                        ? self.preferredExpandedMessageSize(scope: .primary)
                        : PetOverlayLayout.compactMessageSize
                )
            }
            .store(in: &cancellables)

        runningActivityInteractionState.$isMessageExpanded
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] expanded in
                guard let self else { return }
                self.applyRunningActivitySize(
                    expanded
                        ? self.preferredExpandedMessageSize(scope: .running)
                        : PetOverlayLayout.compactMessageSize
                )
            }
            .store(in: &cancellables)

        interactionState.$isQuickChatPresented
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] presented in
                guard let self else { return }
                if presented {
                    self.applyQuickChatSize(self.preferredQuickChatMessageSize())
                } else {
                    self.dismissTaskInspector()
                }
                self.updateMessageVisibility()
                self.updateRunningActivityVisibility()
                self.updateActivityVisibility()
            }
            .store(in: &cancellables)

        fileWorkbenchStore.$isPresented
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                self?.updateMessageVisibility()
                self?.updateFileWorkbenchVisibility()
                self?.updateRunningActivityVisibility()
                self?.updateActivityVisibility()
            }
            .store(in: &cancellables)

        interactionState.$selectedQuickChatResourceID
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self, self.interactionState.isQuickChatPresented else { return }
                self.dismissTaskInspector()
                self.applyQuickChatSize(self.preferredQuickChatMessageSize())
                self.updateRunningActivityVisibility()
                self.updateActivityVisibility()
            }
            .store(in: &cancellables)

        interactionState.$isTranslationPresented
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self, self.interactionState.isQuickChatPresented else { return }
                self.dismissTaskInspector()
                self.applyQuickChatSize(self.preferredQuickChatMessageSize())
                self.updateRunningActivityVisibility()
                self.updateActivityVisibility()
            }
            .store(in: &cancellables)

        interactionState.$isNotepadPresented
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self, self.interactionState.isQuickChatPresented else { return }
                self.dismissTaskInspector()
                self.applyQuickChatSize(self.preferredQuickChatMessageSize())
                self.updateRunningActivityVisibility()
                self.updateActivityVisibility()
            }
            .store(in: &cancellables)

        if let model {
            Publishers.CombineLatest(
                model.$projects.map(\.count).removeDuplicates(),
                preferences.$favoriteProjectIDs.map(\.count).removeDuplicates()
            )
            .receive(on: RunLoop.main)
            .sink { [weak self] _, _ in
                guard let self,
                      self.interactionState.isQuickChatPresented,
                      self.interactionState.selectedQuickChatResourceID == nil else { return }
                self.applyQuickChatSize(self.preferredQuickChatMessageSize())
                self.updateRunningActivityVisibility()
                self.updateActivityVisibility()
            }
            .store(in: &cancellables)
        }

        activityInteractionState.$selectedActivityID
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self, self.activityInteractionState.isMessageExpanded else { return }
                self.applyActivitySize(self.preferredExpandedMessageSize(scope: .primary))
            }
            .store(in: &cancellables)

        activityInteractionState.$inspectedTaskActivity
            .map { $0?.id }
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self, self.activityInteractionState.isMessageExpanded else { return }
                self.applyActivitySize(self.preferredExpandedMessageSize(scope: .primary))
            }
            .store(in: &cancellables)

        runningActivityInteractionState.$selectedActivityID
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self,
                      self.runningActivityInteractionState.isMessageExpanded else { return }
                self.applyRunningActivitySize(
                    self.preferredExpandedMessageSize(scope: .running)
                )
            }
            .store(in: &cancellables)

        runningActivityInteractionState.$inspectedTaskActivity
            .map { $0?.id }
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self,
                      self.runningActivityInteractionState.isMessageExpanded else { return }
                self.applyRunningActivitySize(
                    self.preferredExpandedMessageSize(scope: .running)
                )
            }
            .store(in: &cancellables)

        preferences.$size
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] size in self?.updatePetSize(size) }
            .store(in: &cancellables)

        preferences.$showAcrossSpaces
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.applyCollectionBehavior() }
            .store(in: &cancellables)

        preferences.$resetPositionRequestID
            .dropFirst()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.placeDefault() }
            .store(in: &cancellables)

        NotificationCenter.default.publisher(for: NSApplication.didChangeScreenParametersNotification)
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.clampToVisibleScreen() }
            .store(in: &cancellables)
    }

    private func applyCollectionBehavior() {
        let behavior: NSWindow.CollectionBehavior = preferences.showAcrossSpaces
            ? [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
            : [.managed, .ignoresCycle]
        window?.collectionBehavior = behavior
        messagePanel.collectionBehavior = behavior
        fileWorkbenchPanel.collectionBehavior = behavior
        runningActivityPanel.collectionBehavior = behavior
        activityPanel.collectionBehavior = behavior
    }

    private func updatePetSize(_ size: Double) {
        guard let window else { return }
        let origin = window.frame.origin
        isProgrammaticMove = true
        window.setFrame(NSRect(x: origin.x, y: origin.y, width: size, height: size), display: true)
        isProgrammaticMove = false
        clampToVisibleScreen()
    }

    func updateMessageVisibility() {
        guard window?.isVisible == true,
              interactionState.isQuickChatPresented else {
            messagePanel.orderOut(nil)
            return
        }
        attachMessagePanelIfNeeded()
        positionMessagePanel()
        messagePanel.orderFrontRegardless()
    }

    private func updateFileWorkbenchVisibility() {
        guard window?.isVisible == true, fileWorkbenchStore.isPresented else {
            fileWorkbenchPanel.orderOut(nil)
            return
        }
        attachFileWorkbenchPanelIfNeeded()
        positionFileWorkbenchPanel()
        fileWorkbenchPanel.makeKeyAndOrderFront(nil)
    }

    func updateRunningActivityVisibility() {
        guard window?.isVisible == true,
              store.activities.contains(where: Self.isRunningActivity) else {
            runningActivityInteractionState.isMessageExpanded = false
            runningActivityPanel.orderOut(nil)
            positionActivityPanel()
            return
        }
        applyPanelSize(
            runningActivityInteractionState.isMessageExpanded
                ? preferredExpandedMessageSize(scope: .running)
                : PetOverlayLayout.compactMessageSize,
            to: runningActivityPanel
        )
        attachRunningActivityPanelIfNeeded()
        positionRunningActivityPanel()
        runningActivityPanel.orderFrontRegardless()
        positionActivityPanel()
    }

    func updateActivityVisibility() {
        guard window?.isVisible == true,
              activityInteractionState.inspectedTaskActivity != nil
                || store.presentation.primaryActivity.map({ !Self.isRunningActivity($0) }) == true else {
            activityInteractionState.isMessageExpanded = false
            activityPanel.orderOut(nil)
            return
        }
        applyPanelSize(
            activityInteractionState.isMessageExpanded
                ? preferredExpandedMessageSize(scope: .primary)
                : PetOverlayLayout.compactMessageSize,
            to: activityPanel
        )
        attachActivityPanelIfNeeded()
        positionActivityPanel()
        activityPanel.orderFrontRegardless()
    }

    private func attachMessagePanelIfNeeded() {
        guard let window, messagePanel.parent !== window else { return }
        window.addChildWindow(messagePanel, ordered: .above)
    }

    private func attachFileWorkbenchPanelIfNeeded() {
        guard let window, fileWorkbenchPanel.parent !== window else { return }
        window.addChildWindow(fileWorkbenchPanel, ordered: .above)
    }

    private func attachRunningActivityPanelIfNeeded() {
        guard let window, runningActivityPanel.parent !== window else { return }
        window.addChildWindow(runningActivityPanel, ordered: .above)
    }

    private func attachActivityPanelIfNeeded() {
        guard let window, activityPanel.parent !== window else { return }
        window.addChildWindow(activityPanel, ordered: .above)
    }

    private static func isRunningActivity(_ activity: PetActivity) -> Bool {
        activity.kind == .working || activity.kind == .reviewing
    }

    private func presentTaskInspector(
        selection: TaskReplySelection,
        service: any MessageTaskGraphServicing
    ) {
        guard let model, let petWindow = window else { return }
        dismissTaskInspector()

        let size = NSSize(width: 720, height: 620)
        let panel = PetTaskInspectorPanel(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false
        panel.collectionBehavior = messagePanel.collectionBehavior
        panel.level = NSWindow.Level(rawValue: activityPanel.level.rawValue + 1)
        panel.onCancel = { [weak self] in self?.dismissTaskInspector() }
        panel.contentView = NSHostingView(
            rootView: PetQuickChatTaskInspectorView(
                selection: selection,
                service: service,
                onClose: { [weak self] in self?.dismissTaskInspector() }
            )
            .environmentObject(model)
        )

        let layout = PetTaskInspectorPlacement.layout(
            size: size,
            conversationFrame: messagePanel.frame,
            visibleFrame: (messagePanel.screen ?? petWindow.screen ?? NSScreen.main)?.visibleFrame ?? .zero
        )
        messagePanel.setFrameOrigin(layout.conversationOrigin)
        panel.setFrameOrigin(layout.inspectorOrigin)
        positionRunningActivityPanel()
        positionActivityPanel()
        messagePanel.addChildWindow(panel, ordered: .above)
        panel.makeKeyAndOrderFront(nil)
        taskInspectorPanel = panel
    }

    private func dismissTaskInspector() {
        guard let panel = taskInspectorPanel else { return }
        if panel.parent === messagePanel {
            messagePanel.removeChildWindow(panel)
        }
        panel.orderOut(nil)
        taskInspectorPanel = nil
        if messagePanel.isVisible {
            positionMessagePanel()
            positionRunningActivityPanel()
            positionActivityPanel()
        }
    }

}
