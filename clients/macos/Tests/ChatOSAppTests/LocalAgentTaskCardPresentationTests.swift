import ChatOSCore
import Testing
@testable import ChatOSApp

struct LocalAgentTaskCardPresentationTests {
    @Test
    func hidesMemorySyncStatusWhenRunHasNoOutstandingRecords() {
        let status = LocalAgentMemorySyncStatus(
            runID: "run-1",
            pendingCount: 0,
            failedCount: 0
        )

        #expect(LocalAgentTaskMemorySyncPresentation(status: status) == nil)
    }

    @Test
    func presentsPendingMemorySyncForTheRun() throws {
        let presentation = try #require(LocalAgentTaskMemorySyncPresentation(status: .init(
            runID: "run-1",
            pendingCount: 3,
            failedCount: 0
        )))

        #expect(presentation.kind == .pending)
        #expect(presentation.count == 3)
        #expect(presentation.errorCode == nil)
    }

    @Test
    func presentsFailuresBeforePendingRecordsAndKeepsErrorCode() throws {
        let presentation = try #require(LocalAgentTaskMemorySyncPresentation(status: .init(
            runID: "run-1",
            pendingCount: 4,
            failedCount: 2,
            lastErrorCode: "memory_sync_failed"
        )))

        #expect(presentation.kind == .failed)
        #expect(presentation.count == 2)
        #expect(presentation.errorCode == "memory_sync_failed")
    }
}
