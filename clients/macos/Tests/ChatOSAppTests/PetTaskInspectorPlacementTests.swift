import AppKit
import ChatOSCore
import Testing
@testable import ChatOSApp

@Suite("Pet animation activity policy")
struct PetAnimationActivityPolicyTests {
    @Test("keeps a visible pet animated while another app is active")
    func animatesVisiblePetWithoutApplicationActivation() {
        #expect(PetAnimationActivityPolicy.isActive(
            isPetVisible: true,
            isScreenAwake: true
        ))
    }

    @Test("pauses animation when the pet is hidden or the screen sleeps")
    func pausesWhenNotVisible() {
        #expect(!PetAnimationActivityPolicy.isActive(
            isPetVisible: false,
            isScreenAwake: true
        ))
        #expect(!PetAnimationActivityPolicy.isActive(
            isPetVisible: true,
            isScreenAwake: false
        ))
    }
}

@Suite("Pet quick action sizing")
@MainActor
struct PetQuickActionSizingTests {
    @Test("uses conversation size while quick translation is open")
    func translationUsesConversationSize() {
        let size = PetOverlaySizing.quickChatMessageSize(
            selectedResourceID: nil,
            isTranslationPresented: true,
            isNotepadPresented: false,
            resources: []
        )

        #expect(size == PetOverlayLayout.quickTranslationSize)
    }

    @Test("uses a dedicated editor size while quick notepad is open")
    func notepadUsesEditorSize() {
        let size = PetOverlaySizing.quickChatMessageSize(
            selectedResourceID: nil,
            isTranslationPresented: false,
            isNotepadPresented: true,
            resources: []
        )

        #expect(size == PetOverlayLayout.quickNotepadSize)
    }

    @Test("reserves launcher rows for quick translation and notepad")
    func launcherIncludesQuickActionRows() {
        let resource = PetQuickChatResource(
            id: "contact:1",
            sourceID: "1",
            kind: .contact,
            title: "Translator",
            subtitle: nil,
            conversationID: "conversation"
        )
        let size = PetOverlaySizing.quickChatMessageSize(
            selectedResourceID: nil,
            isTranslationPresented: false,
            isNotepadPresented: false,
            resources: [resource]
        )

        #expect(size.height == 300)
    }
}

@Suite("Pet task inspector placement")
struct PetTaskInspectorPlacementTests {
    @Test("places the inspector to the left and shifts the conversation when needed")
    func placesToTheLeft() {
        let layout = PetTaskInspectorPlacement.layout(
            size: NSSize(width: 720, height: 620),
            conversationFrame: NSRect(x: 680, y: 200, width: 420, height: 500),
            visibleFrame: NSRect(x: 0, y: 0, width: 1500, height: 1000)
        )

        #expect(layout.conversationOrigin == NSPoint(x: 740, y: 200))
        #expect(layout.inspectorOrigin == NSPoint(x: 8, y: 80))
    }

    @Test("keeps an already suitable conversation position and clamps vertically")
    func preservesSuitableConversationPosition() {
        let layout = PetTaskInspectorPlacement.layout(
            size: NSSize(width: 720, height: 620),
            conversationFrame: NSRect(x: 900, y: 600, width: 420, height: 500),
            visibleFrame: NSRect(x: 0, y: 0, width: 1500, height: 1000)
        )

        #expect(layout.conversationOrigin == NSPoint(x: 900, y: 600))
        #expect(layout.inspectorOrigin == NSPoint(x: 168, y: 372))
    }
}

@Suite("Pet stacked panel placement")
struct PetStackedPanelPlacementTests {
    @Test("places a new activity card directly above the open conversation")
    func stacksAboveConversation() {
        let origin = PetStackedPanelPlacement.origin(
            size: NSSize(width: 310, height: 112),
            anchorFrame: NSRect(x: 500, y: 180, width: 420, height: 500),
            visibleFrame: NSRect(x: 0, y: 0, width: 1_440, height: 900)
        )

        #expect(origin == NSPoint(x: 555, y: 690))
    }

    @Test("keeps the activity card visible when there is no room above")
    func fallsBackBesideConversation() {
        let origin = PetStackedPanelPlacement.origin(
            size: NSSize(width: 400, height: 470),
            anchorFrame: NSRect(x: 500, y: 180, width: 420, height: 650),
            visibleFrame: NSRect(x: 0, y: 0, width: 1_440, height: 900)
        )

        #expect(origin == NSPoint(x: 930, y: 360))
    }

    @Test("stacks a new message in its own card above running work")
    func stacksMessageAboveRunningWork() {
        let visibleFrame = NSRect(x: 0, y: 0, width: 1_440, height: 1_200)
        let runningOrigin = PetStackedPanelPlacement.origin(
            size: NSSize(width: 400, height: 260),
            anchorFrame: NSRect(x: 620, y: 80, width: 112, height: 112),
            visibleFrame: visibleFrame
        )
        let runningFrame = NSRect(
            origin: runningOrigin,
            size: NSSize(width: 400, height: 260)
        )
        let messageOrigin = PetStackedPanelPlacement.origin(
            size: NSSize(width: 400, height: 235),
            anchorFrame: runningFrame,
            visibleFrame: visibleFrame
        )

        #expect(messageOrigin.y == runningFrame.maxY + 10)
        #expect(messageOrigin.x == runningOrigin.x)
    }
}

@Suite("Pet activity panel scopes")
struct PetActivityPanelScopeTests {
    @Test("running work and new messages are routed to different panels")
    func separatesRunningWorkFromMessages() {
        let running = PetActivity(
            id: "running",
            source: .taskRunner,
            kind: .working,
            title: "正在执行"
        )
        let completion = PetActivity(
            id: "completion",
            source: .chat,
            kind: .succeeded,
            title: "AI 已完成本轮任务"
        )

        #expect(PetMessageActivityScope.running.contains(running))
        #expect(!PetMessageActivityScope.running.contains(completion))
        #expect(PetMessageActivityScope.primary.contains(completion))
        #expect(!PetMessageActivityScope.primary.contains(running))
    }
}
