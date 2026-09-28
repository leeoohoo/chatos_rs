@preconcurrency import AppKit
import ChatOSCore
import Foundation

@MainActor
extension PetOverlayWindowController {
    func applyQuickChatSize(_ size: NSSize) {
        applyPanelSize(size, to: messagePanel)
        if messagePanel.isVisible {
            positionMessagePanel()
            positionRunningActivityPanel()
            positionActivityPanel()
        }
    }

    func applyRunningActivitySize(_ size: NSSize) {
        applyPanelSize(size, to: runningActivityPanel)
        if runningActivityPanel.isVisible {
            positionRunningActivityPanel()
            positionActivityPanel()
        }
    }

    func applyActivitySize(_ size: NSSize) {
        applyPanelSize(size, to: activityPanel)
        if activityPanel.isVisible {
            positionActivityPanel()
        }
    }

    func applyPanelSize(_ size: NSSize, to panel: NSPanel) {
        let currentSize = panel.contentView?.frame.size
            ?? panel.contentRect(forFrameRect: panel.frame).size
        let sizeMatches = abs(currentSize.width - size.width) <= 0.5
            && abs(currentSize.height - size.height) <= 0.5
        let constraintsMatch = abs(panel.contentMinSize.width - size.width) <= 0.5
            && abs(panel.contentMinSize.height - size.height) <= 0.5
            && abs(panel.contentMaxSize.width - size.width) <= 0.5
            && abs(panel.contentMaxSize.height - size.height) <= 0.5
        guard !sizeMatches || !constraintsMatch else { return }
        panel.contentMinSize = size
        panel.contentMaxSize = size
        panel.setContentSize(size)
        panel.contentView?.frame = NSRect(origin: .zero, size: size)
    }

    func preferredExpandedMessageSize(scope: PetMessageActivityScope) -> NSSize {
        PetOverlaySizing.expandedMessageSize(
            scope: scope,
            store: store,
            interactionState: scope == .primary
                ? activityInteractionState
                : runningActivityInteractionState
        )
    }

    func preferredQuickChatMessageSize() -> NSSize {
        PetOverlaySizing.quickChatMessageSize(
            selectedResourceID: interactionState.selectedQuickChatResourceID,
            isTranslationPresented: interactionState.isTranslationPresented,
            isNotepadPresented: interactionState.isNotepadPresented,
            resources: model?.petQuickChatResources ?? []
        )
    }

    func beginMovingPet() {
        isDraggingPet = true
        lastDragOriginX = window?.frame.origin.x
        interactionState.isDragging = true
    }

    func finishPetInteraction(didDrag: Bool) {
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
                interactionState.isTranslationPresented = false
                interactionState.isNotepadPresented = false
                translationViewModel.cancel()
            }
        }
        updateMessageVisibility()
        updateRunningActivityVisibility()
        updateActivityVisibility()
    }

    func positionMessagePanel() {
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

    func positionFileWorkbenchPanel() {
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

    func positionRunningActivityPanel() {
        guard let petWindow = window,
              let screen = petWindow.screen ?? NSScreen.main else { return }
        runningActivityPanel.setFrameOrigin(PetStackedPanelPlacement.origin(
            size: runningActivityPanel.frame.size,
            anchorFrame: activityBaseAnchorFrame(petWindow: petWindow),
            visibleFrame: screen.visibleFrame
        ))
    }

    func positionActivityPanel() {
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
        if fileWorkbenchPanel.isVisible { return fileWorkbenchPanel.frame }
        if messagePanel.isVisible { return messagePanel.frame }
        return petWindow.frame
    }

    func restoreOrPlaceDefault() {
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

    func placeDefault() {
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

    func clampToVisibleScreen() {
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

    func savePosition() {
        guard let origin = window?.frame.origin else { return }
        UserDefaults.standard.set(origin.x, forKey: PetOverlayPositionKey.x)
        UserDefaults.standard.set(origin.y, forKey: PetOverlayPositionKey.y)
    }
}
