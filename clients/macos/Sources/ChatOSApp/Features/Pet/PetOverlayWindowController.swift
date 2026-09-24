@preconcurrency import AppKit
import ChatOSCore
import Combine
import SwiftUI

@MainActor
final class PetOverlayWindowController: NSWindowController, NSWindowDelegate {

    private let store: PetOverlayStore
    private let preferences: PetPreferencesStore
    private weak var model: AppModel?
    private let messagePanel: NSPanel
    private let activityPanel: NSPanel
    private let runningActivityPanel: NSPanel
    private let fileWorkbenchPanel: NSPanel
    private let fileWorkbenchStore: PetFileWorkbenchStore
    private let interactionState = PetOverlayInteractionState()
    private let activityInteractionState = PetOverlayInteractionState()
    private let runningActivityInteractionState = PetOverlayInteractionState()
    private let onOpen: (PetActivity) -> Void
    private var taskInspectorPanel: NSPanel?
    private var cancellables = Set<AnyCancellable>()
    private var isProgrammaticMove = false
    private var isDraggingPet = false
    private var lastDragOriginX: CGFloat?
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
        dismissTaskInspector()
        fileWorkbenchStore.open(request)
        updateMessageVisibility()
        updateFileWorkbenchVisibility()
        updateRunningActivityVisibility()
        updateActivityVisibility()
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

    private func updateMessageVisibility() {
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

    private func updateRunningActivityVisibility() {
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

    private func updateActivityVisibility() {
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

    private func applyQuickChatSize(_ size: NSSize) {
        applyPanelSize(size, to: messagePanel)
        if messagePanel.isVisible {
            positionMessagePanel()
            positionRunningActivityPanel()
            positionActivityPanel()
        }
    }

    private func applyRunningActivitySize(_ size: NSSize) {
        applyPanelSize(size, to: runningActivityPanel)
        if runningActivityPanel.isVisible {
            positionRunningActivityPanel()
            positionActivityPanel()
        }
    }

    private func applyActivitySize(_ size: NSSize) {
        applyPanelSize(size, to: activityPanel)
        if activityPanel.isVisible {
            positionActivityPanel()
        }
    }

    private func applyPanelSize(_ size: NSSize, to panel: NSPanel) {
        let currentSize = panel.contentView?.frame.size
            ?? panel.contentRect(forFrameRect: panel.frame).size
        let sizeMatches = abs(currentSize.width - size.width) <= 0.5
            && abs(currentSize.height - size.height) <= 0.5
        let constraintsMatch = abs(panel.contentMinSize.width - size.width) <= 0.5
            && abs(panel.contentMinSize.height - size.height) <= 0.5
            && abs(panel.contentMaxSize.width - size.width) <= 0.5
            && abs(panel.contentMaxSize.height - size.height) <= 0.5
        guard !sizeMatches || !constraintsMatch else {
            return
        }
        panel.contentMinSize = size
        panel.contentMaxSize = size
        panel.setContentSize(size)
        panel.contentView?.frame = NSRect(origin: .zero, size: size)
    }

    private func preferredExpandedMessageSize(scope: PetMessageActivityScope) -> NSSize {
        PetOverlaySizing.expandedMessageSize(
            scope: scope,
            store: store,
            interactionState: scope == .primary
                ? activityInteractionState
                : runningActivityInteractionState
        )
    }

    private func preferredQuickChatMessageSize() -> NSSize {
        PetOverlaySizing.quickChatMessageSize(
            selectedResourceID: interactionState.selectedQuickChatResourceID,
            resources: model?.petQuickChatResources ?? []
        )
    }

    private func beginMovingPet() {
        isDraggingPet = true
        lastDragOriginX = window?.frame.origin.x
        interactionState.isDragging = true
    }

    private func finishPetInteraction(didDrag: Bool) {
        interactionState.isDragging = false
        isDraggingPet = false
        lastDragOriginX = nil
        if didDrag {
            clampToVisibleScreen()
            savePosition()
        } else if fileWorkbenchStore.isPresented {
            fileWorkbenchStore.requestDismiss()
        } else {
            interactionState.isQuickChatPresented.toggle()
            if !interactionState.isQuickChatPresented {
                interactionState.selectedQuickChatResourceID = nil
            }
        }
        updateMessageVisibility()
        updateRunningActivityVisibility()
        updateActivityVisibility()
    }

    private func positionMessagePanel() {
        guard let petWindow = window,
              let screen = petWindow.screen ?? NSScreen.main else { return }
        let visible = screen.visibleFrame
        let bubble = messagePanel.frame.size
        let pet = petWindow.frame
        let preferredAbove = pet.maxY + 10
        let y = preferredAbove + bubble.height <= visible.maxY
            ? preferredAbove
            : pet.minY - bubble.height - 10
        let centeredX = pet.midX - bubble.width / 2
        let x = min(max(centeredX, visible.minX + 8), visible.maxX - bubble.width - 8)
        messagePanel.setFrameOrigin(NSPoint(x: x, y: y))
    }

    private func positionFileWorkbenchPanel() {
        guard let petWindow = window,
              let screen = petWindow.screen ?? NSScreen.main else { return }
        let visible = screen.visibleFrame
        let workbench = fileWorkbenchPanel.frame.size
        let pet = petWindow.frame
        let preferredAbove = pet.maxY + 12
        let y = preferredAbove + workbench.height <= visible.maxY
            ? preferredAbove
            : max(visible.minY + 8, pet.minY - workbench.height - 12)
        let centeredX = pet.midX - workbench.width / 2
        let x = min(max(centeredX, visible.minX + 8), visible.maxX - workbench.width - 8)
        fileWorkbenchPanel.setFrameOrigin(NSPoint(x: x, y: y))
    }

    private func positionRunningActivityPanel() {
        guard let petWindow = window,
              let screen = petWindow.screen ?? NSScreen.main else { return }
        runningActivityPanel.setFrameOrigin(PetStackedPanelPlacement.origin(
            size: runningActivityPanel.frame.size,
            anchorFrame: activityBaseAnchorFrame(petWindow: petWindow),
            visibleFrame: screen.visibleFrame
        ))
    }

    private func positionActivityPanel() {
        guard let petWindow = window,
              let screen = petWindow.screen ?? NSScreen.main else { return }
        let anchorFrame = runningActivityPanel.isVisible
            ? runningActivityPanel.frame
            : activityBaseAnchorFrame(petWindow: petWindow)
        activityPanel.setFrameOrigin(PetStackedPanelPlacement.origin(
            size: activityPanel.frame.size,
            anchorFrame: anchorFrame,
            visibleFrame: screen.visibleFrame
        ))
    }

    private func activityBaseAnchorFrame(petWindow: NSWindow) -> NSRect {
        if fileWorkbenchPanel.isVisible {
            return fileWorkbenchPanel.frame
        }
        if messagePanel.isVisible {
            return messagePanel.frame
        }
        return petWindow.frame
    }

    private func restoreOrPlaceDefault() {
        let defaults = UserDefaults.standard
        guard defaults.object(forKey: PetOverlayPositionKey.x) != nil,
              defaults.object(forKey: PetOverlayPositionKey.y) != nil,
              let window else {
            placeDefault()
            return
        }
        window.setFrameOrigin(NSPoint(
            x: defaults.double(forKey: PetOverlayPositionKey.x),
            y: defaults.double(forKey: PetOverlayPositionKey.y)
        ))
        clampToVisibleScreen()
    }

    private func placeDefault() {
        guard let window, let screen = NSScreen.main else { return }
        let visible = screen.visibleFrame
        isProgrammaticMove = true
        window.setFrameOrigin(NSPoint(
            x: visible.maxX - window.frame.width - 30,
            y: visible.minY + 48
        ))
        isProgrammaticMove = false
        savePosition()
        positionMessagePanel()
        positionFileWorkbenchPanel()
        positionRunningActivityPanel()
        positionActivityPanel()
    }

    private func clampToVisibleScreen() {
        guard let window else { return }
        let center = NSPoint(x: window.frame.midX, y: window.frame.midY)
        let screen = NSScreen.screens.first(where: { $0.visibleFrame.contains(center) })
            ?? window.screen
            ?? NSScreen.main
        guard let visible = screen?.visibleFrame else { return }
        let x = min(max(window.frame.minX, visible.minX), visible.maxX - window.frame.width)
        let y = min(max(window.frame.minY, visible.minY), visible.maxY - window.frame.height)
        isProgrammaticMove = true
        window.setFrameOrigin(NSPoint(x: x, y: y))
        isProgrammaticMove = false
        savePosition()
        positionMessagePanel()
        positionFileWorkbenchPanel()
        positionRunningActivityPanel()
        positionActivityPanel()
    }

    private func savePosition() {
        guard let origin = window?.frame.origin else { return }
        UserDefaults.standard.set(origin.x, forKey: PetOverlayPositionKey.x)
        UserDefaults.standard.set(origin.y, forKey: PetOverlayPositionKey.y)
    }
}
