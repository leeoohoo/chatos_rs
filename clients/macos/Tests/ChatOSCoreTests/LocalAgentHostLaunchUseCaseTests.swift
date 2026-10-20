import Testing
@testable import ChatOSCore

@Suite("Local Agent Host launch")
struct LocalAgentHostLaunchUseCaseTests {
    private enum TestError: Error {
        case restoreFailed
        case startFailed
    }

    @MainActor
    private final class RecordingPort: LocalAgentHostLaunchPort {
        var operations: [String] = []
        var restoredServices = true
        var restoreError: Error?
        var startError: Error?

        func resetEventHub() {
            operations.append("reset-event-hub")
        }

        func startHost(ownerUserID: String) throws {
            operations.append("start:\(ownerUserID)")
            if let startError { throw startError }
        }

        func restoreLocalServices(ownerUserID: String) throws -> Bool {
            operations.append("restore:\(ownerUserID)")
            if let restoreError { throw restoreError }
            return restoredServices
        }

        func configureWorkspace(ownerUserID: String) {
            operations.append("workspace:\(ownerUserID)")
        }

        func configureProjectConversation(ownerUserID: String) {
            operations.append("project-conversation:\(ownerUserID)")
        }

        func configureNotepad(ownerUserID: String) {
            operations.append("notepad:\(ownerUserID)")
        }

        func configureRemoteConnectionMetadata(ownerUserID: String) {
            operations.append("remote-metadata:\(ownerUserID)")
        }
    }

    @Test @MainActor
    func preservesTheExistingLocalFirstLaunchOrder() async throws {
        let port = RecordingPort()
        var transitions: [LocalAgentHostLaunchTransition] = []

        try await LocalAgentHostLaunchUseCase().execute(
            ownerUserID: "owner-1",
            port: port,
            isCurrent: { true },
            onTransition: { transitions.append($0) }
        )

        #expect(port.operations == [
            "reset-event-hub",
            "start:owner-1",
            "restore:owner-1",
            "workspace:owner-1",
            "project-conversation:owner-1",
            "notepad:owner-1",
            "remote-metadata:owner-1",
        ])
        #expect(transitions == [
            .handshaking,
            .synchronizing,
            .restoredServicesReady,
            .workspaceReady,
        ])
    }

    @Test @MainActor
    func missingLocalSnapshotContinuesToWorkspaceConfiguration() async throws {
        let port = RecordingPort()
        port.restoreError = TestError.restoreFailed
        var transitions: [LocalAgentHostLaunchTransition] = []

        try await LocalAgentHostLaunchUseCase().execute(
            ownerUserID: "owner-1",
            port: port,
            isCurrent: { true },
            onTransition: { transitions.append($0) }
        )

        #expect(port.operations.contains("workspace:owner-1"))
        #expect(!transitions.contains(.restoredServicesReady))
        #expect(transitions.last == .workspaceReady)
    }

    @Test @MainActor
    func staleLifecycleStopsBeforeTheNextStage() async {
        let port = RecordingPort()
        var checks = 0

        await #expect(throws: CancellationError.self) {
            try await LocalAgentHostLaunchUseCase().execute(
                ownerUserID: "owner-1",
                port: port,
                isCurrent: {
                    checks += 1
                    return checks < 3
                },
                onTransition: { _ in }
            )
        }

        #expect(port.operations == ["reset-event-hub", "start:owner-1"])
    }

    @Test @MainActor
    func hostStartFailureRemainsFatal() async {
        let port = RecordingPort()
        port.startError = TestError.startFailed

        await #expect(throws: TestError.startFailed) {
            try await LocalAgentHostLaunchUseCase().execute(
                ownerUserID: "owner-1",
                port: port,
                isCurrent: { true },
                onTransition: { _ in }
            )
        }

        #expect(port.operations == ["reset-event-hub", "start:owner-1"])
    }
}
