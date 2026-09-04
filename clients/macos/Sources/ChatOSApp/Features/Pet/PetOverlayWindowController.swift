@preconcurrency import AppKit
import ChatOSCore
import Combine
import SwiftUI

@MainActor
final class PetOverlayWindowController: NSWindowController, NSWindowDelegate {
    private enum PositionKey {
        static let x = "ChatOS.pet.position.x"
        static let y = "ChatOS.pet.position.y"
    }

    private enum Layout {
        static let compactMessageSize = NSSize(width: 310, height: 112)
        static let expandedMessageWidth: CGFloat = 400
        static let quickChatWidth: CGFloat = 420
        static let quickChatConversationHeight: CGFloat = 500
        // Keep the process inspector compact and stable. Its timeline already scrolls,
        // so reserving space for several hypothetical nodes only creates empty space
        // for the common one-node case and makes the panel appear to jump in size.
        static let taskProcessMessageSize = NSSize(width: 580, height: 300)
    }

    private let store: PetOverlayStore
    private let preferences: PetPreferencesStore
    private weak var model: AppModel?
    private let messagePanel: NSPanel
    private let interactionState = PetOverlayInteractionState()
    private let onOpen: (PetActivity) -> Void
    private var taskInspectorPanel: NSPanel?
    private var cancellables = Set<AnyCancellable>()
    private var isProgrammaticMove = false
    private var isDraggingPet = false
    private var lastDragOriginX: CGFloat?

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

        let petPanel = Self.makePanel(size: NSSize(width: preferences.size, height: preferences.size))
        self.messagePanel = Self.makePanel(
            size: Layout.compactMessageSize,
            acceptsKeyboardInput: true
        )
        super.init(window: petPanel)

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
                content: PetMessageView(
                    store: store,
                    interactionState: interactionState,
                    preferences: preferences,
                    approvalViewModel: approvalViewModel,
                    onOpen: onOpen,
                    onRetry: onRetry,
                    onCancel: onCancel,
                    onLoadTask: onLoadTask,
                    onLoadPrompt: onLoadPrompt,
                    onSubmitPrompt: onSubmitPrompt,
                    onCancelPrompt: onCancelPrompt,
                    onInspectTaskReply: { [weak self] selection, service in
                        self?.presentTaskInspector(selection: selection, service: service)
                    }
                )
            )
        )
        messageHostingView.sizingOptions = []
        messageHostingView.frame = NSRect(origin: .zero, size: Layout.compactMessageSize)
        messagePanel.contentView = messageHostingView
        applyMessageSize(Layout.compactMessageSize)

        applyCollectionBehavior()
        restoreOrPlaceDefault()
        bind()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    func setVisible(_ visible: Bool) {
        guard let window else { return }
        if visible {
            window.orderFrontRegardless()
            updateMessageVisibility()
        } else {
            dismissTaskInspector()
            window.orderOut(nil)
            messagePanel.orderOut(nil)
        }
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
    }

    func windowDidChangeScreen(_ notification: Notification) {
        clampToVisibleScreen()
    }

    private static func makePanel(
        size: NSSize,
        acceptsKeyboardInput: Bool = false
    ) -> NSPanel {
        let contentRect = NSRect(origin: .zero, size: size)
        let styleMask: NSWindow.StyleMask = [.borderless, .nonactivatingPanel]
        let panel: NSPanel = acceptsKeyboardInput
            ? PetMessagePanel(
                contentRect: contentRect,
                styleMask: styleMask,
                backing: .buffered,
                defer: false
            )
            : NSPanel(
                contentRect: contentRect,
                styleMask: styleMask,
                backing: .buffered,
                defer: false
            )
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false
        panel.level = .floating
        panel.animationBehavior = .utilityWindow
        panel.isMovable = true
        panel.isMovableByWindowBackground = false
        // SwiftUI's TextEditor does not reliably advertise that a borderless
        // non-activating panel needs key status. The interactive message panel
        // therefore takes key status on click while the pet panel stays passive.
        panel.becomesKeyOnlyIfNeeded = !acceptsKeyboardInput
        return panel
    }

    private func bind() {
        store.$presentation
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self else { return }
                if self.interactionState.isMessageExpanded {
                    self.applyMessageSize(self.preferredExpandedMessageSize())
                }
                self.updateMessageVisibility()
            }
            .store(in: &cancellables)

        interactionState.$isMessageExpanded
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] expanded in
                guard let self else { return }
                self.applyMessageSize(
                    expanded ? self.preferredExpandedMessageSize() : Layout.compactMessageSize
                )
            }
            .store(in: &cancellables)

        interactionState.$isQuickChatPresented
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] presented in
                guard let self else { return }
                if presented {
                    self.applyMessageSize(self.preferredQuickChatMessageSize())
                } else {
                    self.dismissTaskInspector()
                    self.applyMessageSize(
                        self.interactionState.isMessageExpanded
                            ? self.preferredExpandedMessageSize()
                            : Layout.compactMessageSize
                    )
                }
                self.updateMessageVisibility()
            }
            .store(in: &cancellables)

        interactionState.$selectedQuickChatResourceID
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self, self.interactionState.isQuickChatPresented else { return }
                self.dismissTaskInspector()
                self.applyMessageSize(self.preferredQuickChatMessageSize())
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
                self.applyMessageSize(self.preferredQuickChatMessageSize())
            }
            .store(in: &cancellables)
        }

        interactionState.$selectedActivityID
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self, self.interactionState.isMessageExpanded else { return }
                self.applyMessageSize(self.preferredExpandedMessageSize())
            }
            .store(in: &cancellables)

        interactionState.$inspectedTaskActivity
            .map { $0?.id }
            .removeDuplicates()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self, self.interactionState.isMessageExpanded else { return }
                self.applyMessageSize(self.preferredExpandedMessageSize())
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
        guard window?.isVisible == true else {
            messagePanel.orderOut(nil)
            return
        }
        if !interactionState.isQuickChatPresented,
           store.presentation.primaryActivity == nil,
           interactionState.inspectedTaskActivity == nil {
            interactionState.isMessageExpanded = false
            messagePanel.orderOut(nil)
        } else {
            attachMessagePanelIfNeeded()
            positionMessagePanel()
            messagePanel.orderFrontRegardless()
        }
    }

    private func attachMessagePanelIfNeeded() {
        guard let window, messagePanel.parent !== window else { return }
        window.addChildWindow(messagePanel, ordered: .above)
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
        panel.level = NSWindow.Level(rawValue: messagePanel.level.rawValue + 1)
        panel.onCancel = { [weak self] in self?.dismissTaskInspector() }
        panel.contentView = NSHostingView(
            rootView: PetQuickChatTaskInspectorView(
                selection: selection,
                service: service,
                onClose: { [weak self] in self?.dismissTaskInspector() }
            )
            .environmentObject(model)
        )

        panel.setFrameOrigin(taskInspectorOrigin(
            size: size,
            petWindow: petWindow
        ))
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
    }

    private func taskInspectorOrigin(size: NSSize, petWindow: NSWindow) -> NSPoint {
        let screen = messagePanel.screen ?? petWindow.screen ?? NSScreen.main
        let visible = screen?.visibleFrame ?? .zero
        return PetTaskInspectorPlacement.origin(
            size: size,
            conversationFrame: messagePanel.frame,
            petFrame: petWindow.frame,
            visibleFrame: visible
        )
    }

    private func applyMessageSize(_ size: NSSize) {
        let currentSize = messagePanel.contentView?.frame.size ?? messagePanel.contentRect(forFrameRect: messagePanel.frame).size
        let sizeMatches = abs(currentSize.width - size.width) <= 0.5
            && abs(currentSize.height - size.height) <= 0.5
        let constraintsMatch = abs(messagePanel.contentMinSize.width - size.width) <= 0.5
            && abs(messagePanel.contentMinSize.height - size.height) <= 0.5
            && abs(messagePanel.contentMaxSize.width - size.width) <= 0.5
            && abs(messagePanel.contentMaxSize.height - size.height) <= 0.5
        guard !sizeMatches || !constraintsMatch else {
            return
        }
        messagePanel.contentMinSize = size
        messagePanel.contentMaxSize = size
        messagePanel.setContentSize(size)
        messagePanel.contentView?.frame = NSRect(origin: .zero, size: size)
        if messagePanel.isVisible {
            positionMessagePanel()
        }
    }

    private func preferredExpandedMessageSize() -> NSSize {
        if interactionState.inspectedTaskActivity != nil {
            return Layout.taskProcessMessageSize
        }
        let selectedActivity = interactionState.selectedActivityID.flatMap { selectedID in
            store.activities.first(where: { $0.id == selectedID })
        }
        guard let activity = selectedActivity ?? store.presentation.primaryActivity else {
            return NSSize(width: Layout.expandedMessageWidth, height: 250)
        }
        let completedTaskCount = store.activities.filter {
            $0.kind == .succeeded
                && ($0.source == .taskRunner || $0.source == .taskBoard)
        }.count
        let height: CGFloat
        switch activity.kind {
        case .waitingForApproval:
            height = store.presentation.attentionCount > 1 ? 470 : 390
        case .blocked, .failed:
            height = store.presentation.activeWorkCount > 0 ? 390 : 345
        case .working, .reviewing:
            let taskCount = max(1, store.presentation.activeWorkCount)
            let runningHeight = min(320, 148 + CGFloat(min(taskCount - 1, 3)) * 57)
            height = min(430, runningHeight + (completedTaskCount > 0 ? 110 : 0))
        case .waitingForUser:
            height = 470
        case .succeeded, .cancelled:
            let baseHeight: CGFloat = store.presentation.activeWorkCount > 0 ? 320 : 235
            height = min(430, baseHeight + (completedTaskCount > 1 ? 110 : 0))
        }
        return NSSize(width: Layout.expandedMessageWidth, height: height)
    }

    private func preferredQuickChatMessageSize() -> NSSize {
        guard interactionState.selectedQuickChatResourceID == nil else {
            return NSSize(
                width: Layout.quickChatWidth,
                height: Layout.quickChatConversationHeight
            )
        }

        let resources = model?.petQuickChatResources ?? []
        let rowCount = max(1, resources.count)
        let rowHeight = CGFloat(rowCount) * 56
        let rowSpacing = CGFloat(max(0, rowCount - 1)) * 8
        let favoriteHintHeight: CGFloat = resources.allSatisfy { $0.kind == .contact } ? 38 : 0
        let notificationFooterHeight: CGFloat = store.presentation.primaryActivity == nil ? 0 : 50
        let contentHeight = 78 + rowHeight + rowSpacing + favoriteHintHeight + notificationFooterHeight
        return NSSize(
            width: Layout.quickChatWidth,
            height: min(410, max(190, contentHeight))
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
        } else {
            interactionState.isMessageExpanded = false
            interactionState.selectedActivityID = nil
            interactionState.inspectedTaskActivity = nil
            interactionState.isQuickChatPresented.toggle()
            if !interactionState.isQuickChatPresented {
                interactionState.selectedQuickChatResourceID = nil
            }
        }
        updateMessageVisibility()
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

    private func restoreOrPlaceDefault() {
        let defaults = UserDefaults.standard
        guard defaults.object(forKey: PositionKey.x) != nil,
              defaults.object(forKey: PositionKey.y) != nil,
              let window else {
            placeDefault()
            return
        }
        window.setFrameOrigin(NSPoint(
            x: defaults.double(forKey: PositionKey.x),
            y: defaults.double(forKey: PositionKey.y)
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
    }

    private func savePosition() {
        guard let origin = window?.frame.origin else { return }
        UserDefaults.standard.set(origin.x, forKey: PositionKey.x)
        UserDefaults.standard.set(origin.y, forKey: PositionKey.y)
    }
}

struct PetTaskInspectorPlacement {
    static func origin(
        size: NSSize,
        conversationFrame: NSRect,
        petFrame: NSRect,
        visibleFrame: NSRect
    ) -> NSPoint {
        let centeredX = conversationFrame.midX - size.width / 2
        let x = min(
            max(centeredX, visibleFrame.minX + 8),
            visibleFrame.maxX - size.width - 8
        )
        let aboveConversation = conversationFrame.maxY + 10
        if aboveConversation + size.height <= visibleFrame.maxY - 8 {
            return NSPoint(x: x, y: aboveConversation)
        }
        return NSPoint(x: x, y: petFrame.minY - size.height - 10)
    }
}

private final class PetMessagePanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func sendEvent(_ event: NSEvent) {
        if event.type == .leftMouseDown, !isKeyWindow {
            makeKey()
        }
        super.sendEvent(event)
    }
}

private final class PetTaskInspectorPanel: NSPanel {
    var onCancel: (() -> Void)?

    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }

    override func cancelOperation(_ sender: Any?) {
        onCancel?()
    }
}

private struct PetLocalizedRoot<Content: View>: View {
    @ObservedObject var model: AppModel
    let content: Content

    var body: some View {
        content
            .environmentObject(model)
            .environment(\.locale, model.interfaceLocale)
            .environment(\.interfaceFontScale, model.interfaceFontScale)
            .dynamicTypeSize(model.interfaceDynamicTypeSize)
    }
}

@MainActor
private final class PetInteractionHostingView<Content: View>: NSHostingView<Content> {
    private let onInteractionBegan: () -> Void
    private let onInteractionEnded: (Bool) -> Void
    private var dragStartMouseLocation: NSPoint?
    private var dragStartWindowOrigin: NSPoint?
    private var didMoveWindow = false

    init(
        rootView: Content,
        onInteractionBegan: @escaping () -> Void,
        onInteractionEnded: @escaping (Bool) -> Void
    ) {
        self.onInteractionBegan = onInteractionBegan
        self.onInteractionEnded = onInteractionEnded
        super.init(rootView: rootView)
        wantsLayer = true
        layer?.isOpaque = false
        layer?.backgroundColor = NSColor.clear.cgColor
    }

    @available(*, unavailable)
    required init(rootView: Content) {
        fatalError("init(rootView:) is unavailable")
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var mouseDownCanMoveWindow: Bool { false }

    override var isOpaque: Bool { false }

    override func mouseDown(with event: NSEvent) {
        guard let window else {
            super.mouseDown(with: event)
            return
        }
        // Do not use NSWindow.performDrag here. Its modal event-tracking loop
        // prevents SwiftUI's TimelineView from reliably advancing sprite frames
        // until the pointer is released. Moving the panel from mouseDragged keeps
        // the normal run loop alive, so the directional gait animates in real time.
        dragStartMouseLocation = NSEvent.mouseLocation
        dragStartWindowOrigin = window.frame.origin
        didMoveWindow = false
        onInteractionBegan()
    }

    override func mouseDragged(with event: NSEvent) {
        guard let window,
              let startMouseLocation = dragStartMouseLocation,
              let startWindowOrigin = dragStartWindowOrigin else {
            super.mouseDragged(with: event)
            return
        }
        let currentMouseLocation = NSEvent.mouseLocation
        let deltaX = currentMouseLocation.x - startMouseLocation.x
        let deltaY = currentMouseLocation.y - startMouseLocation.y
        window.setFrameOrigin(NSPoint(
            x: startWindowOrigin.x + deltaX,
            y: startWindowOrigin.y + deltaY
        ))
        if !didMoveWindow, hypot(deltaX, deltaY) >= 3 {
            didMoveWindow = true
        }
    }

    override func mouseUp(with event: NSEvent) {
        guard let window,
              let startMouseLocation = dragStartMouseLocation,
              let startWindowOrigin = dragStartWindowOrigin else {
            super.mouseUp(with: event)
            return
        }
        let currentMouseLocation = NSEvent.mouseLocation
        let mouseDistance = hypot(
            currentMouseLocation.x - startMouseLocation.x,
            currentMouseLocation.y - startMouseLocation.y
        )
        let windowDistance = hypot(
            window.frame.origin.x - startWindowOrigin.x,
            window.frame.origin.y - startWindowOrigin.y
        )
        dragStartMouseLocation = nil
        dragStartWindowOrigin = nil
        let completedDrag = didMoveWindow || max(mouseDistance, windowDistance) >= 3
        didMoveWindow = false
        onInteractionEnded(completedDrag)
    }
}
