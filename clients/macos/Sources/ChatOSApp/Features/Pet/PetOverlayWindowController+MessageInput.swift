@preconcurrency import AppKit

@MainActor
extension PetOverlayWindowController {
    func focusMessageInput(_ responder: NSResponder) {
        guard interactionState.isQuickChatPresented,
              let textView = responder as? NSTextView,
              textView.window === messagePanel || textView.window === messageInputPanel else { return }

        if textView.window === messageInputPanel,
           messageInputPanel.isKeyWindow,
           messageInputPanel.firstResponder === responder {
            return
        }

        if applicationBeforeMessageInput == nil,
           NSWorkspace.shared.frontmostApplication?.bundleIdentifier
            != Bundle.main.bundleIdentifier {
            applicationBeforeMessageInput = NSWorkspace.shared.frontmostApplication
        }

        // A regular application is activated on the Space containing its main
        // window. Temporarily becoming an accessory lets this floating input
        // panel activate in the foreground application's Space, which is also
        // where the system input method creates its candidate window.
        if applicationBeforeMessageInput != nil,
           activationPolicyBeforeMessageInput == nil {
            activationPolicyBeforeMessageInput = NSApp.activationPolicy()
            _ = NSApp.setActivationPolicy(.accessory)
        }

        guard pendingMessageInputResponder == nil else { return }
        pendingMessageInputResponder = responder
        DispatchQueue.main.async { [weak self] in
            self?.presentActiveMessageInputIfNeeded()
        }
    }

    func presentActiveMessageInputIfNeeded() {
        guard interactionState.isQuickChatPresented,
              let responder = pendingMessageInputResponder,
              let textView = responder as? NSTextView else { return }

        if textView.window === messagePanel {
            guard messagePanel.isVisible, let contentView = messagePanel.contentView else { return }
            let panelFrame = messagePanel.frame
            dismissTaskInspector()
            if let parent = messagePanel.parent {
                parent.removeChildWindow(messagePanel)
            }
            messagePanel.makeFirstResponder(nil)
            messagePanel.contentView = nil
            messageInputPanel.setFrame(panelFrame, display: false)
            messageInputPanel.contentMinSize = messagePanel.contentMinSize
            messageInputPanel.contentMaxSize = messagePanel.contentMaxSize
            contentView.frame = NSRect(origin: .zero, size: panelFrame.size)
            messageInputPanel.contentView = contentView
            isMessageInputActive = true
        }

        // Apple documents `.canJoinAllApplications` for floating windows and
        // system overlays that must join other apps' window sets, including
        // their full-screen Spaces. Make this panel main/key before activation
        // so AppKit does not select ChatOS's main window on another Space.
        attachMessageInputPanelIfNeeded()
        messageInputPanel.orderFrontRegardless()
        messageInputPanel.makeMain()
        messageInputPanel.makeKeyAndOrderFront(nil)
        if messageInputPanel.isVisible {
            messagePanel.orderOut(nil)
        }

        if NSApp.isActive {
            finishMessageInputActivationIfNeeded()
        } else {
            NSApp.activate(ignoringOtherApps: true)
        }
    }

    func finishMessageInputActivationIfNeeded() {
        guard NSApp.isActive,
              interactionState.isQuickChatPresented,
              let responder = pendingMessageInputResponder,
              let textView = responder as? NSTextView,
              textView.window === messageInputPanel else { return }
        pendingMessageInputResponder = nil
        messageInputPanel.orderFrontRegardless()
        messageInputPanel.makeMain()
        messageInputPanel.makeKeyAndOrderFront(nil)
        messageInputPanel.makeFirstResponder(responder)
    }

    func restoreApplicationAfterMessageInput() {
        let previousApplication = applicationBeforeMessageInput
        let previousActivationPolicy = activationPolicyBeforeMessageInput
        applicationBeforeMessageInput = nil
        activationPolicyBeforeMessageInput = nil
        pendingMessageInputResponder = nil
        if isMessageInputActive {
            messageInputPanel.makeFirstResponder(nil)
            let contentView = messageInputPanel.contentView
            let panelFrame = messageInputPanel.frame
            if let parent = messageInputPanel.parent {
                parent.removeChildWindow(messageInputPanel)
            }
            messageInputPanel.orderOut(nil)
            messageInputPanel.contentView = nil
            messagePanel.setFrame(panelFrame, display: false)
            messagePanel.contentView = contentView
            contentView?.frame = NSRect(origin: .zero, size: panelFrame.size)
            isMessageInputActive = false
            if window?.isVisible == true, interactionState.isQuickChatPresented {
                attachMessagePanelIfNeeded()
                messagePanel.orderFrontRegardless()
            }
        }
        if let previousActivationPolicy,
           NSApp.activationPolicy() != previousActivationPolicy {
            _ = NSApp.setActivationPolicy(previousActivationPolicy)
        }
        guard NSApp.isActive,
              let previousApplication,
              !previousApplication.isTerminated,
              !previousApplication.isActive else { return }
        previousApplication.activate()
    }

    func attachMessageInputPanelIfNeeded() {
        guard let window, messageInputPanel.parent !== window else { return }
        if let parent = messageInputPanel.parent {
            parent.removeChildWindow(messageInputPanel)
        }
        window.addChildWindow(messageInputPanel, ordered: .above)
    }
}
