import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeLocalAgentExecutionWorkspaceStoreTests {
    @Test
    func defaultWorkspaceIsAllocatedOnTheSourceVolume() async throws {
        let fixture = try ExecutionWorkspaceFixture()
        defer { fixture.dispose() }
        let store = NativeLocalAgentExecutionWorkspaceStore()

        let isolated = try await store.bind(
            context: fixture.context(enabledKinds: ["TerminalController"]),
            runID: "run-same-volume"
        )
        let keys: Set<URLResourceKey> = [.volumeIdentifierKey]
        let sourceVolume = try fixture.sourceRoot.resourceValues(forKeys: keys).volumeIdentifier
        let isolatedVolume = try isolated.executionRootURL.resourceValues(forKeys: keys).volumeIdentifier

        #expect(sourceVolume != nil)
        #expect(sourceVolume?.isEqual(isolatedVolume) == true)
        await store.release(runID: "run-same-volume")
    }

    @Test
    func terminalOnlyTaskUsesIndependentCopyOnWriteWorkspace() async throws {
        let fixture = try ExecutionWorkspaceFixture()
        defer { fixture.dispose() }
        let store = NativeLocalAgentExecutionWorkspaceStore(baseURL: fixture.leasesRoot)
        let context = fixture.context(enabledKinds: ["CodeMaintainerRead", "TerminalController"])

        let isolated = try await store.bind(context: context, runID: "run-read-only")
        #expect(isolated.usesIsolatedWorkspace)
        #expect(isolated.executionRootURL != fixture.sourceRoot)
        #expect(try String(contentsOf: isolated.executionRootURL.appendingPathComponent("source.txt")) == "original")

        try "changed in isolated workspace".write(
            to: isolated.executionRootURL.appendingPathComponent("source.txt"),
            atomically: true,
            encoding: .utf8
        )
        try "isolated-index".write(
            to: isolated.executionRootURL.appendingPathComponent(".git/index"),
            atomically: true,
            encoding: .utf8
        )

        #expect(try String(contentsOf: fixture.sourceRoot.appendingPathComponent("source.txt")) == "original")
        #expect(try String(contentsOf: fixture.sourceRoot.appendingPathComponent(".git/index")) == "source-index")
        try await store.verifySourceUnchanged(runID: "run-read-only")
        await store.release(runID: "run-read-only")
        #expect(!FileManager.default.fileExists(atPath: isolated.executionRootURL.path))
    }

    @Test
    func explicitProjectWriteKeepsTheAuthorizedSourceWorkspace() async throws {
        let fixture = try ExecutionWorkspaceFixture()
        defer { fixture.dispose() }
        let store = NativeLocalAgentExecutionWorkspaceStore(baseURL: fixture.leasesRoot)
        let context = fixture.context(
            enabledKinds: ["CodeMaintainerRead", "CodeMaintainerWrite", "TerminalController"]
        )

        let bound = try await store.bind(context: context, runID: "run-writer")

        #expect(!bound.usesIsolatedWorkspace)
        #expect(bound.executionRootURL == fixture.sourceRoot)
    }

    @Test
    func sourceGitMetadataMutationStopsAnIsolatedRun() async throws {
        let fixture = try ExecutionWorkspaceFixture()
        defer { fixture.dispose() }
        let store = NativeLocalAgentExecutionWorkspaceStore(baseURL: fixture.leasesRoot)
        let context = fixture.context(enabledKinds: ["TerminalController"])
        _ = try await store.bind(context: context, runID: "run-guard")
        try "unexpected-source-index".write(
            to: fixture.sourceRoot.appendingPathComponent(".git/index"),
            atomically: true,
            encoding: .utf8
        )

        await #expect(throws: NativeLocalAgentExecutionWorkspaceError.sourceGitMetadataChanged) {
            try await store.verifySourceUnchanged(runID: "run-guard")
        }
    }
}

private struct ExecutionWorkspaceFixture {
    let root: URL
    let sourceRoot: URL
    let leasesRoot: URL

    init() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-execution-workspace-\(UUID().uuidString)", isDirectory: true)
        sourceRoot = root.appendingPathComponent("source", isDirectory: true)
        leasesRoot = root.appendingPathComponent("leases", isDirectory: true)
        try FileManager.default.createDirectory(
            at: sourceRoot.appendingPathComponent(".git/refs/heads", isDirectory: true),
            withIntermediateDirectories: true
        )
        try "original".write(
            to: sourceRoot.appendingPathComponent("source.txt"),
            atomically: true,
            encoding: .utf8
        )
        try "ref: refs/heads/main\n".write(
            to: sourceRoot.appendingPathComponent(".git/HEAD"),
            atomically: true,
            encoding: .utf8
        )
        try "source-index".write(
            to: sourceRoot.appendingPathComponent(".git/index"),
            atomically: true,
            encoding: .utf8
        )
        try "[core]\n\trepositoryformatversion = 0\n".write(
            to: sourceRoot.appendingPathComponent(".git/config"),
            atomically: true,
            encoding: .utf8
        )
        try "0123456789abcdef\n".write(
            to: sourceRoot.appendingPathComponent(".git/refs/heads/main"),
            atomically: true,
            encoding: .utf8
        )
    }

    func context(enabledKinds: Set<String>) -> NativeLocalAgentTaskExecutionContext {
        let workspace = LocalConnectorWorkspace(
            id: "workspace-1",
            alias: "Fixture",
            absoluteRoot: sourceRoot.path,
            fingerprint: "fixture"
        )
        return NativeLocalAgentTaskExecutionContext(
            conversationID: "conversation-1",
            projectID: "project-1",
            applicationContext: .init(
                projectID: "project-1",
                projectName: "Fixture",
                projectRoot: sourceRoot.path
            ),
            resolvedPath: .init(
                workspace: workspace,
                relativePath: ".",
                absoluteURL: sourceRoot,
                logicalPrefix: nil
            ),
            executionRootURL: sourceRoot,
            workspaceScopeID: workspace.id,
            toolAuthorization: .init(
                requiresExecution: true,
                enabledBuiltinKinds: enabledKinds,
                pluginSelectionIDs: [],
                ownedPaths: []
            )
        )
    }

    func dispose() {
        try? FileManager.default.removeItem(at: root)
    }
}
