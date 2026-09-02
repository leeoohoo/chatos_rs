import Foundation
import Testing
@testable import ChatOSApp

@MainActor
struct ScreenshotWorkflowGateTests {
    @Test
    func onlyOneScreenshotWorkflowCanOwnTheProcessAtATime() {
        let gate = ScreenshotWorkflowGate()
        let first = NSObject()
        let second = NSObject()

        #expect(gate.acquire(first))
        #expect(!gate.acquire(first))
        #expect(!gate.acquire(second))

        gate.release(first)
        #expect(gate.acquire(second))
    }

    @Test
    func onlyTheOwnerCanReleaseTheWorkflow() {
        let gate = ScreenshotWorkflowGate()
        let owner = NSObject()
        let outsider = NSObject()

        #expect(gate.acquire(owner))
        gate.release(outsider)
        #expect(!gate.acquire(outsider))
        gate.release(owner)
        #expect(gate.acquire(outsider))
    }
}
