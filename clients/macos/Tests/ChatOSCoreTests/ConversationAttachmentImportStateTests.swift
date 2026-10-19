import Testing
@testable import ChatOSCore

@Suite("ConversationAttachmentImportState")
struct ConversationAttachmentImportStateTests {
    @Test
    func concurrentImportsFinishIndependently() {
        var state = ConversationAttachmentImportState()
        let first = state.begin()
        let second = state.begin()

        let didFinishFirst = state.finish(first)
        #expect(didFinishFirst)
        #expect(!state.accepts(first))
        #expect(state.accepts(second))
        let didFinishSecond = state.finish(second)
        #expect(didFinishSecond)
    }

    @Test
    func resetRejectsAllImportsFromPreviousDraft() {
        var state = ConversationAttachmentImportState()
        let file = state.begin()
        let image = state.begin()

        state.reset()
        let current = state.begin()

        #expect(!state.accepts(file))
        #expect(!state.accepts(image))
        #expect(state.accepts(current))
        #expect(current.generation == file.generation + 1)
    }

    @Test
    func staleCompletionCannotFinishCurrentImport() {
        var state = ConversationAttachmentImportState()
        let stale = state.begin()
        state.reset()
        let current = state.begin()

        let didFinishStale = state.finish(stale)
        #expect(!didFinishStale)
        #expect(state.accepts(current))
    }
}
