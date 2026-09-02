import Testing
@testable import ChatOSApp

struct GlobalHotKeyPressGateTests {
    @Test
    func duplicatePressesAreDebouncedWithoutRequiringAReleaseEvent() {
        var gate = GlobalHotKeyPressGate()

        let firstPress = gate.shouldTrigger(.screenshot, isPressed: true, now: 10)
        let firstRepeat = gate.shouldTrigger(.screenshot, isPressed: true, now: 10.05)
        let secondRepeat = gate.shouldTrigger(.screenshot, isPressed: true, now: 10.20)
        let release = gate.shouldTrigger(.screenshot, isPressed: false, now: 10.21)
        let secondPress = gate.shouldTrigger(.screenshot, isPressed: true, now: 10.36)

        #expect(firstPress)
        #expect(!firstRepeat)
        #expect(!secondRepeat)
        #expect(!release)
        #expect(secondPress)
    }

    @Test
    func shortcutsTrackTheirPressedStateIndependently() {
        var gate = GlobalHotKeyPressGate()

        let screenshotPress = gate.shouldTrigger(.screenshot, isPressed: true, now: 20)
        let clipboardPress = gate.shouldTrigger(.clipboardHistory, isPressed: true, now: 20)
        let screenshotRepeat = gate.shouldTrigger(.screenshot, isPressed: true, now: 20.1)
        let clipboardRelease = gate.shouldTrigger(.clipboardHistory, isPressed: false, now: 20.2)
        let clipboardSecondPress = gate.shouldTrigger(.clipboardHistory, isPressed: true, now: 20.36)

        #expect(screenshotPress)
        #expect(clipboardPress)
        #expect(!screenshotRepeat)
        #expect(!clipboardRelease)
        #expect(clipboardSecondPress)
    }
}
