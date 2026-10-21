@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentPlatformToolWorkerTests: XCTestCase {
    func testBuiltinAuditLimitIsAnExplicitPreviewInsteadOfSilentTruncation() {
        let detail = String(repeating: "路径", count: 9_000)

        let preview = NativeLocalConnectorService.agentBuiltinAuditDetailPreview(detail)

        XCTAssertEqual(preview.count, 16_000)
        XCTAssertTrue(preview.contains("进度详情为预览"))
        XCTAssertTrue(preview.contains("分页读取工具或交付文件"))
        XCTAssertNotEqual(preview, String(detail.prefix(16_000)))
    }

    func testExternalOnlyTaskCanUseUnifiedSkillControlsWithoutBuiltinAccess() throws {
        let authorization = try NativeLocalAgentTaskToolAuthorization.resolve([
            "tool_options": .object([
                "requires_execution": .bool(false), "enabled_builtin_kinds": .array([]),
                "plugin_hints": .array([]), "external_mcp_config_ids": .array([.string("mcp-1")]),
                "plugin_release_snapshots": .array([]),
            ]),
        ])
        XCTAssertTrue(authorization.allows("agent_skill_activate"))
        XCTAssertTrue(authorization.allows("agent_skill_read_resource"))
        XCTAssertFalse(authorization.allows("execute_command"))
        XCTAssertFalse(authorization.allows("read_file"))
        for invalid: LocalAgentJSONValue in [.null, .array([.number(1)]), .array([.string("")]),
            .array([.string("mcp-1"), .string("mcp-1")])] {
            XCTAssertThrowsError(try NativeLocalAgentTaskToolAuthorization.resolve([
                "tool_options": .object(["requires_execution": .bool(false), "enabled_builtin_kinds": .array([]),
                    "external_mcp_config_ids": invalid]),
            ]))
        }
    }
    func testReturnedToolErrorCommitsFailureEvenForSideEffectingTool() async throws {
        let host = PlatformToolHostStub(mode: .oneSideEffectingClaim)
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host), executor: ReturnedErrorPlatformToolExecutor(),
            workerID: "worker-1"
        )
        await worker.configure(ownerUserID: "user-1")
        await worker.wake()
        for _ in 0..<200 {
            if await host.commitCount() > 0 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let outcome = try await host.committedOutcomeType()
        XCTAssertEqual(outcome, .string("failed"))
        await worker.reset()
    }

    func testWaitingForApprovalDoesNotBlockApprovedClaimExecution() async throws {
        let host = PlatformToolHostStub(mode: .oneSideEffectingClaim)
        let executor = CountingPlatformToolExecutor()
        let approvalHandler = BlockingPlatformToolApprovalHandler()
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: executor,
            approvalHandler: approvalHandler,
            workerID: "worker-1",
            activityWindow: .seconds(1)
        )

        await worker.configure(ownerUserID: "user-1")
        for _ in 0..<200 {
            if await approvalHandler.didStart(), await host.commitCount() == 1 { break }
            try await Task.sleep(for: .milliseconds(10))
        }

        let approvalStarted = await approvalHandler.didStart()
        let executionCount = await executor.executionCount()
        let commitCount = await host.commitCount()
        XCTAssertTrue(approvalStarted)
        XCTAssertEqual(executionCount, 1)
        XCTAssertEqual(commitCount, 1)
        await worker.reset()
    }

    func testApprovalWakeDuringActiveResolutionRunsAgainAfterExit() async throws {
        let host = PlatformToolHostStub(mode: .idle)
        let approvalHandler = GatedPlatformToolApprovalHandler()
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: CountingPlatformToolExecutor(),
            approvalHandler: approvalHandler,
            workerID: "worker-1",
            activityWindow: .zero
        )

        await worker.configure(ownerUserID: "user-1")
        for _ in 0..<100 {
            if await approvalHandler.callCount() == 1 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        await worker.wake()
        await approvalHandler.resumeFirstCall()
        for _ in 0..<100 {
            if await approvalHandler.callCount() == 2 { break }
            try await Task.sleep(for: .milliseconds(10))
        }

        let calls = await approvalHandler.callCount()
        XCTAssertEqual(calls, 2)
        await worker.reset()
    }

    func testTaskWriteSessionEnforcesOwnedPathsAndCannotDowngradeScope() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("task-write-ownership-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeMCPCodeWriteStore()
        let scope = NativeMCPCodeWriteScope(workspaceID: "workspace", sessionID: "conversation", runID: "run")
        let paths = ["src", "standalone.txt"]
        let opened = try await store.call(
            name: "open_edit_session", arguments: [:], scope: scope, projectRoot: root, ownedPaths: paths
        )
        let id = try XCTUnwrap(opened.jsonObject?["result"]?.jsonObject?["session_id"]?.jsonString)
        func write(_ path: String) -> NativeJSONValue {
            .object(["kind": .string("write"), "path": .string(path),
                     "content": .string("created"), "expected_sha256": .null])
        }
        for forbidden in ["src-other/file.txt", "other.txt", "../escape.txt"] {
            do {
                _ = try await store.call(
                    name: "stage_edit_batch",
                    arguments: ["session_id": .string(id), "operations": .array([
                        write("src/not-staged.txt"), write(forbidden),
                    ])],
                    scope: scope, projectRoot: root, ownedPaths: paths
                )
                XCTFail("out-of-scope batch must be rejected: \(forbidden)")
            } catch is NativeMCPCodeWriteError {}
        }
        _ = try await store.call(
            name: "stage_edit_batch",
            arguments: ["session_id": .string(id), "operations": .array([
                write("src/nested/file.txt"), write("standalone.txt"),
            ])],
            scope: scope, projectRoot: root, ownedPaths: paths
        )
        do {
            _ = try await store.call(
                name: "commit_edit_session", arguments: ["session_id": .string(id)],
                scope: scope, projectRoot: root
            )
            XCTFail("omitting frozen ownership must not broaden the session")
        } catch is NativeMCPCodeWriteError {}
        let committed = try await store.call(
            name: "commit_edit_session", arguments: ["session_id": .string(id)],
            scope: scope, projectRoot: root, ownedPaths: paths.reversed()
        )
        XCTAssertEqual(committed.jsonObject?["changed_target_count"], .number(2))
        XCTAssertEqual(try String(contentsOf: root.appendingPathComponent("src/nested/file.txt"), encoding: .utf8), "created")
        XCTAssertTrue(FileManager.default.fileExists(atPath: root.appendingPathComponent("standalone.txt").path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("src/not-staged.txt").path))
    }

    func testTaskWriteOwnershipRejectsSymbolicLinkParentsAndParentDeletion() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("task-write-links-\(UUID().uuidString)", isDirectory: true)
        let actual = root.appendingPathComponent("actual", isDirectory: true)
        try FileManager.default.createDirectory(at: actual, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(at: root.appendingPathComponent("owned"), withDestinationURL: actual)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeMCPCodeWriteStore()
        let scope = NativeMCPCodeWriteScope(workspaceID: "workspace", sessionID: "conversation", runID: "run")
        let paths = ["owned", "actual/child.txt"]
        let opened = try await store.call(
            name: "open_edit_session", arguments: [:], scope: scope, projectRoot: root, ownedPaths: paths
        )
        let id = try XCTUnwrap(opened.jsonObject?["result"]?.jsonObject?["session_id"]?.jsonString)
        for operation: NativeJSONValue in [
            .object(["kind": .string("write"), "path": .string("owned/new.txt"),
                     "content": .string("must not write"), "expected_sha256": .null]),
            .object(["kind": .string("delete"), "path": .string("actual"), "expected_sha256": .null]),
        ] {
            do {
                _ = try await store.call(
                    name: "stage_edit_batch",
                    arguments: ["session_id": .string(id), "operations": .array([operation])],
                    scope: scope, projectRoot: root, ownedPaths: paths
                )
                XCTFail("alias writes and parent deletion must be rejected")
            } catch is NativeMCPCodeWriteError {}
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: actual.appendingPathComponent("new.txt").path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: actual.path))
    }

    func testTerminalCommandResolverSkipsBlankCompatibilityField() {
        XCTAssertEqual(
            NativeLocalAgentTerminalCommandResolver.resolve([
                "common": LocalAgentJSONValue.string("  "),
                "command": .string("command -v godot"),
            ]),
            "command -v godot"
        )
        XCTAssertEqual(
            NativeLocalAgentTerminalCommandResolver.resolve([
                "common": NativeJSONValue.string(""),
                "command": .string("git status --short"),
            ]),
            "git status --short"
        )
    }

    func testTerminalCommandResolverPrefersCanonicalCommandOverLegacyAlias() {
        // Approval presentation and both native execution paths share this resolver.
        // An old model may mistake the legacy `common` field for a human summary.
        XCTAssertEqual(
            NativeLocalAgentTerminalCommandResolver.resolve([
                "common": LocalAgentJSONValue.string("只读项目、系统临时 SQLite"),
                "command": .string("python3 -B -m unittest discover -s tests"),
            ]),
            "python3 -B -m unittest discover -s tests"
        )
        XCTAssertEqual(
            NativeLocalAgentTerminalCommandResolver.resolve([
                "common": NativeJSONValue.string("只读项目、系统临时 SQLite"),
                "command": .string("python3 -B -m unittest discover -s tests"),
            ]),
            "python3 -B -m unittest discover -s tests"
        )
    }

    func testTerminalCommandResolverPreservesLegacyOnlyPayloads() {
        XCTAssertEqual(
            NativeLocalAgentTerminalCommandResolver.resolve([
                "common": LocalAgentJSONValue.string("git status --short"),
            ]),
            "git status --short"
        )
        XCTAssertEqual(
            NativeLocalAgentTerminalCommandResolver.resolve([
                "command": NativeJSONValue.string(" \n"),
                "common": .string("git status --short"),
            ]),
            "git status --short"
        )
    }

    func testTerminalPurposeIsTrimmedAndStrictlyBounded() {
        XCTAssertEqual(
            NativeLocalAgentTerminalCommandResolver.purpose([
                "purpose": NativeJSONValue.string("  验证项目能否通过类型检查  "),
            ]),
            "验证项目能否通过类型检查"
        )
        XCTAssertNil(NativeLocalAgentTerminalCommandResolver.purpose([
            "purpose": NativeJSONValue.string(" \n "),
        ]))
        XCTAssertNil(NativeLocalAgentTerminalCommandResolver.purpose([
            "purpose": NativeJSONValue.string(String(repeating: "a", count: 501)),
        ]))
    }

    func testTerminalSchemaPublishesOnlyCanonicalCommandField() {
        guard case let .object(tool)? = NativeMCPTerminalStore.toolDefinitions.first(where: {
            guard case let .object(fields) = $0 else { return false }
            return fields["name"]?.jsonString == "execute_command"
        }), case let .object(parameters)? = tool["inputSchema"],
              case let .object(properties)? = parameters["properties"] else {
            return XCTFail("missing terminal command schema")
        }
        XCTAssertNil(properties["common"])
        XCTAssertNotNil(properties["command"])
        XCTAssertNotNil(properties["purpose"])
        XCTAssertEqual(
            parameters["required"],
            NativeJSONValue.array([.string("purpose"), .string("command")])
        )
    }

    func testWorkerKeepsMonitoringAfterBootstrapPollingBecomesIdle() async throws {
        let host = PlatformToolHostStub(mode: .eventDrivenClaim)
        let eventHub = NativeLocalAgentEventHub(host: host)
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: FailingPlatformToolExecutor(),
            eventHub: eventHub,
            workerID: "worker-1",
            activityWindow: .milliseconds(20)
        )
        await worker.configure(ownerUserID: "user-1")

        for _ in 0..<200 {
            if await host.commitCount() > 0 { break }
            try await Task.sleep(for: .milliseconds(10))
        }

        let commits = await host.commitCount()
        let eventListRequests = await host.eventListRequestCount()
        XCTAssertEqual(commits, 1)
        XCTAssertGreaterThan(eventListRequests, 0)
        await worker.reset()
    }

    func testWorkerReconcilesToolMaterializedShortlyAfterConfigure() async throws {
        let host = PlatformToolHostStub(mode: .delayedClaim(emptyAttempts: 2))
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: FailingPlatformToolExecutor(),
            workerID: "worker-1",
            activityWindow: .seconds(1)
        )
        await worker.configure(ownerUserID: "user-1")

        for _ in 0..<200 {
            if await host.commitCount() == 1 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let commits = await host.commitCount()
        let attempts = await host.claimAttemptCount()
        XCTAssertEqual(commits, 1)
        XCTAssertGreaterThan(attempts, 2)
        await worker.reset()
    }

    func testWorkerStartsEventMonitoringAfterRecoveringPersistedWork() async throws {
        let host = PlatformToolHostStub(mode: .oneSideEffectingClaim)
        let eventHub = NativeLocalAgentEventHub(host: host)
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: FailingPlatformToolExecutor(),
            eventHub: eventHub,
            workerID: "worker-1"
        )
        await worker.configure(ownerUserID: "user-1")

        for _ in 0..<200 {
            if await host.commitCount() > 0,
               await host.eventListRequestCount() > 0 { break }
            try await Task.sleep(for: .milliseconds(10))
        }

        let commits = await host.commitCount()
        let eventListRequests = await host.eventListRequestCount()
        XCTAssertEqual(commits, 1)
        XCTAssertGreaterThan(eventListRequests, 0)
        await worker.reset()
    }

    func testWorkerWakesFromSharedToolBatchEvent() async throws {
        let host = PlatformToolHostStub(mode: .eventDrivenClaim)
        let eventHub = NativeLocalAgentEventHub(host: host)
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: FailingPlatformToolExecutor(),
            eventHub: eventHub,
            workerID: "worker-1",
            activityWindow: .milliseconds(50)
        )
        await worker.configure(ownerUserID: "user-1")
        await worker.wake()

        for _ in 0..<200 {
            if await host.commitCount() > 0 { break }
            try await Task.sleep(for: .milliseconds(10))
        }

        let commits = await host.commitCount()
        let claimAttempts = await host.claimAttemptCount()
        XCTAssertEqual(commits, 1)
        XCTAssertGreaterThanOrEqual(claimAttempts, 2)
        await worker.reset()
    }

    func testWorkerReleasesRunResourcesFromTerminalEvent() async throws {
        let host = PlatformToolHostStub(mode: .terminalEvent("run_succeeded"))
        let eventHub = NativeLocalAgentEventHub(host: host)
        let executor = RecordingPlatformToolExecutor()
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: executor,
            eventHub: eventHub,
            workerID: "worker-1",
            activityWindow: .milliseconds(50)
        )
        await worker.configure(ownerUserID: "user-1")
        await worker.wake()

        for _ in 0..<200 {
            if await executor.releasedRunIDs() == ["run-1"] { break }
            try await Task.sleep(for: .milliseconds(10))
        }

        let releasedRunIDs = await executor.releasedRunIDs()
        let commitCount = await host.commitCount()
        XCTAssertEqual(releasedRunIDs, ["run-1"])
        XCTAssertEqual(commitCount, 0)
        await worker.reset()
    }

    func testWorkerExecutesFourIndependentPlatformClaimsConcurrently() async throws {
        let host = PlatformToolHostStub(mode: .concurrentClaims(4))
        let executor = ConcurrentPlatformToolExecutor(target: 4)
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: executor,
            workerID: "worker-1",
            maximumConcurrentClaims: 4
        )
        await worker.configure(ownerUserID: "user-1")

        for _ in 0..<200 {
            if await executor.startedCount() == 4 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let started = await executor.startedCount()
        XCTAssertEqual(started, 4)
        for _ in 0..<200 {
            if await host.commitCount() == 4 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let commits = await host.commitCount()
        XCTAssertEqual(commits, 4)
        await worker.reset()
    }

    func testWorkerRefillsCapacityWhenClaimArrivesDuringLongRunningExecution() async throws {
        let host = PlatformToolHostStub(mode: .staggeredClaims)
        let executor = ConcurrentPlatformToolExecutor(target: 2)
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: executor,
            workerID: "worker-1",
            maximumConcurrentClaims: 2
        )
        await worker.configure(ownerUserID: "user-1")

        for _ in 0..<200 {
            if await executor.startedCount() == 1 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let firstStartedCount = await executor.startedCount()
        XCTAssertEqual(firstStartedCount, 1)

        await host.makeStaggeredClaimAvailable()
        for _ in 0..<200 {
            if await executor.startedCount() == 2 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let finalStartedCount = await executor.startedCount()
        XCTAssertEqual(finalStartedCount, 2)

        for _ in 0..<200 {
            if await host.commitCount() == 2 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let commits = await host.commitCount()
        XCTAssertEqual(commits, 2)
        await worker.reset()
    }

    func testPlatformToolPollingPolicyUsesOnlyActionableEvents() {
        XCTAssertEqual(NativeLocalAgentPlatformToolPollingPolicy.activityWindow, .seconds(15))
        XCTAssertNil(NativeLocalAgentConcurrencyPolicy.validated(0))
        XCTAssertEqual(NativeLocalAgentConcurrencyPolicy.validated(7), 7)
        XCTAssertNil(NativeLocalAgentConcurrencyPolicy.validated(17))
        XCTAssertTrue(NativeLocalAgentPlatformToolPollingPolicy.shouldWake(forEventTypes: [
            "tool_batch_requested",
        ]))
        XCTAssertTrue(NativeLocalAgentPlatformToolPollingPolicy.shouldWake(forEventTypes: [
            "tool_invocation_approved",
        ]))
        XCTAssertFalse(NativeLocalAgentPlatformToolPollingPolicy.shouldWake(forEventTypes: [
            "tool_invocation_claimed",
            "tool_invocation_completed",
            "run_succeeded",
        ]))
        XCTAssertTrue(
            NativeLocalAgentPlatformToolPollingPolicy.shouldReleaseRunResources(
                forEventType: "run_succeeded"
            )
        )
        XCTAssertTrue(
            NativeLocalAgentPlatformToolPollingPolicy.shouldReleaseRunResources(
                forEventType: "run_failed"
            )
        )
        XCTAssertTrue(
            NativeLocalAgentPlatformToolPollingPolicy.shouldReleaseRunResources(
                forEventType: "run_cancelled"
            )
        )
        XCTAssertFalse(
            NativeLocalAgentPlatformToolPollingPolicy.shouldReleaseRunResources(
                forEventType: "tool_invocation_completed"
            )
        )
    }

    func testCapabilityCatalogPartitionsRustAndNativeTools() {
        let names = NativeLocalAgentPlatformToolCatalog.capabilityTools.compactMap { value in
            guard case let .object(tool) = value,
                  case let .string(name)? = tool["name"] else { return nil as String? }
            return name
        }
        XCTAssertEqual(
            names,
            NativeLocalAgentPlatformToolCatalog.mainChatTaskToolNames + NativeLocalAgentPlatformToolCatalog.mainChatCompanionTools.compactMap { value in
                guard case let .object(tool) = value, case let .string(name)? = tool["name"] else { return nil }
                return name
            }
        )
        XCTAssertTrue(names.contains("local_attachment_read"))
        XCTAssertTrue(names.filter { $0 != "local_attachment_read" }.allSatisfy {
            !NativeLocalAgentPlatformToolCatalog.taskExecutionToolNames.contains($0)
        })
        XCTAssertEqual(
            NativeLocalAgentPlatformToolCatalog.readOnlyToolNames,
            [
                "local_attachment_read", "read_file_raw", "read_file_range", "list_dir",
                "search_text", "read_file", "search_files", "process_poll", "process_log",
                "process_wait", "remote_connection_controller_download_file",
                "remote_connection_controller_list_directory",
                "remote_connection_controller_read_file",
                "remote_connection_controller_test_connection",
                "agent_skill_activate", "agent_skill_read_resource",
                "capability_describe", "capability_search",
                "plugin_installer_search_npm", "plugin_installer_resolve_npm",
                "plugin_installer_inspect_npm", "plugin_installer_inspect_source",
            ]
        )
        XCTAssertEqual(
            NativeLocalAgentPlatformToolCatalog.taskExecutionToolNames,
            Set([
                "read_file_raw", "read_file_range", "list_dir", "search_text", "read_file",
                "search_files", "open_edit_session", "stage_edit_batch",
                "commit_edit_session", "abort_edit_session",
                "execute_command", "process_poll", "process_log", "process_wait",
                "process_write", "process_kill",
                "remote_connection_controller_test_connection",
                "remote_connection_controller_run_command",
                "remote_connection_controller_list_directory",
                "remote_connection_controller_read_file",
                "remote_connection_controller_download_file",
                "remote_connection_controller_upload_file",
                "remote_connection_controller_upload_project_file",
                "local_attachment_read",
                "capability_search", "capability_describe", "agent_skill_activate",
                "agent_skill_read_resource", "capability_invoke",
            ])
        )
        XCTAssertEqual(
            NativeLocalAgentPlatformToolCatalog.approvalExemptToolNames,
            [
                "open_edit_session", "stage_edit_batch", "abort_edit_session",
                "capability_invoke",
                "plugin_installer_install_npm", "plugin_installer_install_source",
            ]
        )
    }

    func testTaskToolAuthorizationEnforcesSelectedCapabilities() throws {
        let readOnly = try NativeLocalAgentTaskToolAuthorization.resolve([
            "tool_options": .object([
                "requires_execution": .bool(false),
                "enabled_builtin_kinds": .array([
                    .string("CodeMaintainerRead"), .string("CodeMaintainerWrite"),
                    .string("TerminalController"),
                ]),
                "plugin_hints": .array([]),
                "plugin_release_snapshots": .array([]),
            ]),
        ])
        XCTAssertTrue(readOnly.allows("read_file"))
        XCTAssertFalse(readOnly.allows("commit_edit_session"))
        XCTAssertFalse(readOnly.allows("execute_command"))
        XCTAssertTrue(readOnly.allows("capability_search"))
        XCTAssertEqual(readOnly.brokerBuiltinCapabilities, [.projectRead])
        let scoped = try NativeLocalAgentTaskToolAuthorization.resolve([
            "owned_paths": .array([.string("src"), .string("docs/report.md")]),
            "tool_options": .object([
                "requires_execution": .bool(true),
                "enabled_builtin_kinds": .array([]),
                "plugin_hints": .array([]),
                "plugin_release_snapshots": .array([]),
            ]),
        ])
        XCTAssertEqual(scoped.ownedPaths, ["docs/report.md", "src"])
        for path in ["../outside", "/absolute", "src//file", "src\\file", "C:/repo", "src/./file"] {
            XCTAssertThrowsError(try NativeLocalAgentTaskToolAuthorization.resolve([
                "owned_paths": .array([.string(path)]),
            ]))
        }

        let execution = try NativeLocalAgentTaskToolAuthorization.resolve([
            "tool_options": .object([
                "requires_execution": .bool(true),
                "enabled_builtin_kinds": .array([
                    .string("CodeMaintainerRead"),
                    .string("CodeMaintainerWrite"),
                    .string("TerminalController"),
                ]),
                "plugin_hints": .array([.object([
                    "plugin_key": .string("browser"),
                    "component_key": .string("browser-cdp-mcp"),
                ])]),
                "plugin_release_snapshots": .array([.object([
                    "plugin_key": .string("browser"),
                    "component_key": .string("browser-cdp-mcp"),
                    "plugin_id": .string("plugin-1"),
                    "release_id": .string("release-1"),
                    "version": .string("1.2.3"),
                    "artifact_sha256": .string(String(repeating: "a", count: 64)),
                ])]),
            ]),
        ])
        XCTAssertTrue(execution.allows("commit_edit_session"))
        XCTAssertTrue(execution.allows("execute_command"))
        XCTAssertTrue(execution.allows("capability_search"))
        XCTAssertEqual(
            execution.brokerBuiltinCapabilities,
            [.projectRead, .projectWrite, .terminal]
        )
        let browserSelection = NativeLocalAgentTaskToolAuthorization.pluginSelectionID(
            pluginKey: "browser",
            componentKey: "browser-cdp-mcp"
        )
        XCTAssertEqual(execution.pluginSelectionIDs, [browserSelection])
        XCTAssertEqual(
            execution.pluginReleaseSnapshots[browserSelection]?.releaseID,
            "release-1"
        )

        XCTAssertThrowsError(try NativeLocalAgentTaskToolAuthorization.resolve([
            "tool_options": .object([
                "requires_execution": .bool(true),
                "enabled_builtin_kinds": .array([]),
                "plugin_hints": .array([.object(["plugin_key": .string("browser")])]),
                "plugin_release_snapshots": .array([]),
            ]),
        ]))
    }

    func testExecutorDispatchesTaskExecutionProjectTool() async throws {
        let projectExecutor = RecordingProjectToolExecutor()
        let executor = NativeLocalAgentPlatformToolExecutor(
            host: PlatformToolHostStub(mode: .idle),
            attachmentRootURL: FileManager.default.temporaryDirectory,
            projectTools: projectExecutor
        )
        let invocation = LocalAgentToolInvocationRecord(
            invocationID: "invocation-1",
            runID: "run-1",
            batchID: "batch-1",
            callID: "call-1",
            toolName: "read_file_raw",
            executionRoute: "platform",
            arguments: .object(["path": .string("README.md")]),
            sideEffecting: false,
            requiresApproval: false,
            approvalStatus: "not_required",
            approvalDecidedBy: nil,
            approvalReason: nil,
            approvalDecidedAtUnixMs: nil,
            status: "running",
            result: nil,
            error: nil,
            version: 1,
            claimToken: "token-1",
            claimUntilUnixMs: 2,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )

        let result = try await executor.execute(ownerUserID: "user-1", invocation: invocation)

        XCTAssertEqual(result, .object(["dispatched": .bool(true)]))
        let recorded = await projectExecutor.recordedInvocation()
        XCTAssertEqual(recorded?.toolName, "read_file_raw")
    }

    func testExecutorReleasesProjectResourcesForRun() async {
        let projectExecutor = RecordingProjectToolExecutor()
        let executor = NativeLocalAgentPlatformToolExecutor(
            host: PlatformToolHostStub(mode: .idle),
            attachmentRootURL: FileManager.default.temporaryDirectory,
            projectTools: projectExecutor
        )

        await executor.release(runID: "run-1")

        let releasedRunIDs = await projectExecutor.releasedRunIDs()
        XCTAssertEqual(releasedRunIDs, ["run-1"])
    }

    func testAttachmentVaultResolvesBoundedContentAndRejectsTampering() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-attachment-test-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let vault = NativeLocalAgentAttachmentVault(rootURL: root)
        let data = Data("hello local attachment".utf8)
        let spec = try XCTUnwrap(vault.authorize(
            [.init(
                id: "attachment-1",
                name: "notes.txt",
                mimeType: "text/plain",
                kind: .file,
                origin: .pastedText,
                data: data
            )],
            ownerUserID: "owner/a",
            conversationID: "conversation/a"
        ).first)
        let record = attachmentRecord(spec)

        let previewURL = try vault.previewURL(
            record,
            ownerUserID: "owner/a",
            conversationID: "conversation/a"
        )
        XCTAssertTrue(previewURL.isFileURL)
        XCTAssertEqual(try Data(contentsOf: previewURL), data)

        let first = try await vault.resolve(
            record,
            ownerUserID: "owner/a",
            conversationID: "conversation/a",
            offset: 0,
            limit: 5
        )
        XCTAssertEqual(first.content, "hello")
        XCTAssertEqual(first.encoding, "utf-8")
        XCTAssertEqual(first.nextOffset, 5)

        let end = try await vault.resolve(
            record,
            ownerUserID: "owner/a",
            conversationID: "conversation/a",
            offset: UInt64(data.count),
            limit: 5
        )
        XCTAssertEqual(end.content, "")
        XCTAssertNil(end.nextOffset)

        do {
            _ = try await vault.resolve(
                record,
                ownerUserID: "owner/b",
                conversationID: "conversation/a",
                offset: 0,
                limit: 5
            )
            XCTFail("Cross-owner attachment read must be rejected")
        } catch {
        }

        let file = try XCTUnwrap(try FileManager.default.subpathsOfDirectory(atPath: root.path)
            .map { root.appendingPathComponent($0) }
            .first(where: { !$0.hasDirectoryPath && $0.lastPathComponent.count == 36 }))
        try Data("HELLO local attachment".utf8).write(to: file, options: .atomic)
        do {
            _ = try await vault.resolve(
                record,
                ownerUserID: "owner/a",
                conversationID: "conversation/a",
                offset: 0,
                limit: 5
            )
            XCTFail("Tampered attachment must be rejected")
        } catch {
            XCTAssertEqual(error as? NativeLocalAgentAttachmentVaultError, .integrityMismatch)
        }
    }

    func testTypedClientAlwaysExcludesRustReservedTools() async throws {
        let host = PlatformToolHostStub(mode: .idle)
        let client = NativeLocalAgentToolClient(host: host)
        let claim = try await client.claimNext(
            ownerUserID: "user-1",
            workerID: "worker-1",
            excludeToolNames: ["another_reserved_tool"]
        )
        XCTAssertNil(claim)
        let command = try await host.lastCommand()
        guard case let .array(excluded)? = command["exclude_tool_names"] else {
            return XCTFail("missing exclude_tool_names")
        }
        let names = excluded.compactMap { value -> String? in
            guard case let .string(name) = value else { return nil }
            return name
        }
        XCTAssertEqual(Set(names), Set([
            "another_reserved_tool",
            "list_tasks",
            "get_task",
            "create_task",
            "create_tasks_with_prerequisites",
            "cancel_task",
            "wait_for_task_completion",
            "get_task_dependency_graph",
            "resolve_task_followup",
        ]))
    }

    func testTypedClientRenewsExactClaimIdentity() async throws {
        let host = PlatformToolHostStub(mode: .renewingClaim(renewalAccepted: true))
        let client = NativeLocalAgentToolClient(host: host)
        let claimed = try await client.claimNext(
            ownerUserID: "user-1",
            workerID: "worker-1",
            leaseDurationMilliseconds: 1_000
        )
        let claim = try XCTUnwrap(claimed)

        let renewed = try await client.renew(
            ownerUserID: "user-1",
            claim: claim,
            leaseDurationMilliseconds: 2_000
        )
        XCTAssertTrue(renewed)
        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("renew_tool_claim"))
        XCTAssertEqual(command["owner_user_id"], .string("user-1"))
        XCTAssertEqual(command["invocation_id"], .string("invocation-1"))
        XCTAssertEqual(command["claim_token"], .string("claim-token-1"))
        XCTAssertEqual(command["expected_version"], .number(2))
        XCTAssertEqual(command["lease_duration_ms"], .number(2_000))
    }

    func testTypedClientListsAndDecidesDurableToolApproval() async throws {
        let host = PlatformToolHostStub(mode: .onePendingApproval)
        let client = NativeLocalAgentToolClient(host: host)

        let pending = try await client.pendingApprovals(ownerUserID: "user-1", limit: 1)
        let invocation = try XCTUnwrap(pending.first)
        XCTAssertEqual(invocation.toolName, "commit_edit_session")
        XCTAssertEqual(invocation.approvalStatus, "pending")

        let result = try await client.decideApproval(
            ownerUserID: "user-1",
            invocation: invocation,
            approve: true,
            decidedBy: "macos-local-approval",
            reason: "Approved in the local client."
        )
        XCTAssertEqual(result.invocation.approvalStatus, "approved")
        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("decide_tool_approval"))
        XCTAssertEqual(command["expected_version"], .number(2))
        XCTAssertEqual(command["decision"], .string("approve"))
    }

    func testWorkerCommitsSideEffectingExecutionErrorAsNeedsReview() async throws {
        let host = PlatformToolHostStub(mode: .oneSideEffectingClaim)
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: FailingPlatformToolExecutor(),
            workerID: "worker-1"
        )
        await worker.configure(ownerUserID: "user-1")

        var outcomeType: LocalAgentJSONValue?
        for _ in 0..<100 where outcomeType == nil {
            outcomeType = try await host.committedOutcomeType()
            if outcomeType == nil {
                try await Task.sleep(for: .milliseconds(10))
            }
        }
        XCTAssertEqual(outcomeType, .string("needs_review"))
        await worker.reset()
    }

    func testWorkerReturnsRecoverableSideEffectingValidationErrorToModel() async throws {
        let host = PlatformToolHostStub(mode: .oneSideEffectingClaim)
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: RecoverablePlatformToolExecutor(),
            workerID: "worker-1"
        )
        await worker.configure(ownerUserID: "user-1")

        var outcomeType: LocalAgentJSONValue?
        for _ in 0..<100 where outcomeType == nil {
            outcomeType = try await host.committedOutcomeType()
            if outcomeType == nil {
                try await Task.sleep(for: .milliseconds(10))
            }
        }
        XCTAssertEqual(outcomeType, .string("failed"))
        await worker.reset()
    }

    func testWorkerCommitsSharedPreflightDenialWithoutExecutingNativeTool() async throws {
        let host = PlatformToolHostStub(mode: .preflightDeniedClaim)
        let executor = CountingPlatformToolExecutor()
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: executor,
            workerID: "worker-1"
        )
        await worker.configure(ownerUserID: "user-1")

        var outcomeType: LocalAgentJSONValue?
        for _ in 0..<100 where outcomeType == nil {
            outcomeType = try await host.committedOutcomeType()
            if outcomeType == nil {
                try await Task.sleep(for: .milliseconds(10))
            }
        }
        XCTAssertEqual(outcomeType, .string("failed"))
        let executionCount = await executor.executionCount()
        XCTAssertEqual(executionCount, 0)
        await worker.reset()
    }

    func testWorkerRenewsLongRunningClaimUntilCommit() async throws {
        let host = PlatformToolHostStub(mode: .renewingClaim(renewalAccepted: true))
        let executor = DelayedPlatformToolExecutor(delay: .milliseconds(350))
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: executor,
            workerID: "worker-1",
            claimLeaseDurationMilliseconds: 1_000,
            claimHeartbeatInterval: .milliseconds(50)
        )
        await worker.configure(ownerUserID: "user-1")

        for _ in 0..<200 {
            if await host.commitCount() > 0 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let commits = await host.commitCount()
        let renewals = await host.renewalCount()
        XCTAssertEqual(commits, 1)
        XCTAssertGreaterThanOrEqual(renewals, 2)
        await worker.reset()
    }

    func testWorkerRetriesTransientRenewalFailuresBeforeLeaseExpiry() async throws {
        let host = PlatformToolHostStub(mode: .renewingClaimAfterTransientFailures(2))
        let executor = DelayedPlatformToolExecutor(delay: .milliseconds(700))
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: executor,
            workerID: "worker-1",
            claimLeaseDurationMilliseconds: 1_000,
            claimHeartbeatInterval: .milliseconds(50)
        )
        await worker.configure(ownerUserID: "user-1")

        for _ in 0..<200 {
            if await host.commitCount() > 0 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let commits = await host.commitCount()
        let renewals = await host.renewalCount()
        XCTAssertEqual(commits, 1)
        XCTAssertGreaterThanOrEqual(renewals, 3)
        await worker.reset()
    }

    func testWorkerDoesNotCommitAfterClaimRenewalIsRejected() async throws {
        let host = PlatformToolHostStub(mode: .renewingClaim(renewalAccepted: false))
        let executor = DelayedPlatformToolExecutor(delay: .seconds(2))
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: executor,
            workerID: "worker-1",
            claimLeaseDurationMilliseconds: 1_000,
            claimHeartbeatInterval: .milliseconds(25)
        )
        await worker.configure(ownerUserID: "user-1")

        for _ in 0..<100 {
            if await host.renewalCount() > 0 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        try await Task.sleep(for: .milliseconds(100))
        let renewals = await host.renewalCount()
        let commits = await host.commitCount()
        let cancelled = await executor.wasCancelled()
        XCTAssertEqual(renewals, 1)
        XCTAssertEqual(commits, 0)
        XCTAssertTrue(cancelled)
        await worker.reset()
    }

    func testWorkerResetStopsClaimHeartbeat() async throws {
        let host = PlatformToolHostStub(mode: .renewingClaim(renewalAccepted: true))
        let executor = DelayedPlatformToolExecutor(delay: .seconds(2))
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: executor,
            workerID: "worker-1",
            claimLeaseDurationMilliseconds: 1_000,
            claimHeartbeatInterval: .milliseconds(25)
        )
        await worker.configure(ownerUserID: "user-1")

        for _ in 0..<100 {
            if await host.renewalCount() >= 2 { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        await worker.reset()
        let countAfterReset = await host.renewalCount()
        try await Task.sleep(for: .milliseconds(100))
        let finalRenewalCount = await host.renewalCount()
        let commits = await host.commitCount()
        XCTAssertEqual(finalRenewalCount, countAfterReset)
        XCTAssertEqual(commits, 0)
    }

    private func attachmentRecord(
        _ spec: LocalAgentConversationAttachmentSpec
    ) -> LocalAgentConversationAttachmentRecord {
        .init(
            attachmentID: spec.attachmentID,
            conversationID: "conversation/a",
            turnID: "turn-1",
            messageID: "message-1",
            ordinal: 1,
            displayName: spec.displayName,
            mediaType: spec.mediaType,
            byteSize: spec.byteSize,
            sha256: spec.sha256,
            authorizedLocalRef: spec.authorizedLocalRef,
            metadata: spec.metadata,
            createdAtUnixMs: 1
        )
    }
}

private actor RecordingProjectToolExecutor: NativeLocalAgentProjectToolExecuting {
    private var invocation: LocalAgentToolInvocationRecord?
    private var releasedRuns: [String] = []

    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
        self.invocation = invocation
        return .object(["dispatched": .bool(true)])
    }

    func recordedInvocation() -> LocalAgentToolInvocationRecord? { invocation }

    func release(runID: String) async {
        releasedRuns.append(runID)
    }

    func releasedRunIDs() -> [String] { releasedRuns }

    func reset() async {}
}

private struct FailingPlatformToolExecutor: NativeLocalAgentPlatformToolExecuting {
    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
        throw CocoaError(.fileReadCorruptFile)
    }
}

private struct RecoverablePlatformToolExecutor: NativeLocalAgentPlatformToolExecuting {
    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
        throw NativeLocalAgentPlatformToolError.projectToolRejected(
            "文本匹配数量不符合预期：期望 4，实际 3"
        )
    }
}

private struct ReturnedErrorPlatformToolExecutor: NativeLocalAgentPlatformToolExecuting {
    func execute(ownerUserID: String, invocation: LocalAgentToolInvocationRecord) async throws -> LocalAgentJSONValue {
        .object(["is_error": .bool(true), "content": .object(["error": .string("skill rejected")])])
    }
}

private actor CountingPlatformToolExecutor: NativeLocalAgentPlatformToolExecuting {
    private var count = 0

    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
        count += 1
        return .object(["executed": .bool(true)])
    }

    func executionCount() -> Int { count }
}

private actor BlockingPlatformToolApprovalHandler: NativeLocalAgentToolApprovalHandling {
    private var started = false

    func resolveNextPending(ownerUserID: String) async throws -> Bool {
        started = true
        try await Task.sleep(for: .seconds(60))
        return false
    }

    func didStart() -> Bool { started }
}

private actor GatedPlatformToolApprovalHandler: NativeLocalAgentToolApprovalHandling {
    private var calls = 0
    private var firstCallContinuation: CheckedContinuation<Void, Never>?

    func resolveNextPending(ownerUserID: String) async throws -> Bool {
        calls += 1
        if calls == 1 {
            await withCheckedContinuation { continuation in
                firstCallContinuation = continuation
            }
        }
        return false
    }

    func callCount() -> Int { calls }

    func resumeFirstCall() {
        firstCallContinuation?.resume()
        firstCallContinuation = nil
    }
}

private actor RecordingPlatformToolExecutor: NativeLocalAgentPlatformToolExecuting {
    private var releasedRuns: [String] = []

    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
        .object(["executed": .bool(true)])
    }

    func release(runID: String) async {
        releasedRuns.append(runID)
    }

    func releasedRunIDs() -> [String] { releasedRuns }
}

private actor DelayedPlatformToolExecutor: NativeLocalAgentPlatformToolExecuting {
    private let delay: Duration
    private var cancelled = false

    init(delay: Duration) {
        self.delay = delay
    }

    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
        do {
            try await Task.sleep(for: delay)
            return .object(["completed": .bool(true)])
        } catch is CancellationError {
            cancelled = true
            throw CancellationError()
        }
    }

    func wasCancelled() -> Bool { cancelled }
}

private actor ConcurrentPlatformToolExecutor: NativeLocalAgentPlatformToolExecuting {
    private let target: Int
    private var started = 0
    private var waiters: [CheckedContinuation<Void, Never>] = []

    init(target: Int) {
        self.target = target
    }

    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
        started += 1
        if started == target {
            let continuations = waiters
            waiters.removeAll()
            continuations.forEach { $0.resume() }
        } else if started < target {
            await withCheckedContinuation { continuation in
                waiters.append(continuation)
            }
        }
        return .object(["invocation_id": .string(invocation.invocationID)])
    }

    func startedCount() -> Int { started }
}

private actor PlatformToolHostStub: LocalAgentHostClientServicing {
    enum Mode {
        case idle
        case oneSideEffectingClaim
        case preflightDeniedClaim
        case onePendingApproval
        case renewingClaim(renewalAccepted: Bool)
        case renewingClaimAfterTransientFailures(Int)
        case eventDrivenClaim
        case terminalEvent(String)
        case concurrentClaims(Int)
        case staggeredClaims
        case delayedClaim(emptyAttempts: Int)
    }

    private let mode: Mode
    private var didClaim = false
    private var commands: [Data] = []
    private var renewals = 0
    private var commits = 0
    private var claimAttempts = 0
    private var eventDelivered = false
    private var eventListRequests = 0
    private var concurrentClaimCount = 0
    private var staggeredClaimAvailable = false

    init(mode: Mode) {
        self.mode = mode
    }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        switch object?["type"] as? String {
        case "get_event_cursor":
            guard supportsEventStream else {
                throw CocoaError(.featureUnsupported)
            }
            return try json(["type": "event_cursor", "cursor": 0])
        case "wait_events":
            guard supportsEventStream else {
                throw CocoaError(.featureUnsupported)
            }
            eventListRequests += 1
            guard !eventDelivered else {
                return try json(["type": "events", "events": [], "next_cursor": 1])
            }
            try await Task.sleep(for: .milliseconds(100))
            eventDelivered = true
            let eventType = if case let .terminalEvent(value) = mode {
                value
            } else {
                "tool_batch_requested"
            }
            return try json([
                "type": "events",
                "events": [[
                    "cursor": 1,
                    "event_id": "event-tool-batch",
                    "run_id": "run-1",
                    "event_type": eventType,
                    "created_at_unix_ms": 1,
                ]],
                "next_cursor": 1,
            ])
        case "claim_next_tool":
            claimAttempts += 1
            if case let .concurrentClaims(limit) = mode {
                guard concurrentClaimCount < limit else {
                    return try json(["type": "tool_claim", "claim": NSNull()])
                }
                concurrentClaimCount += 1
                return try json([
                    "type": "tool_claim",
                    "claim": [
                        "worker_id": "worker-1",
                        "claim_token": "claim-token-\(concurrentClaimCount)",
                        "invocation": invocation(
                            status: "running",
                            version: 2,
                            index: concurrentClaimCount
                        ),
                    ],
                ])
            }
            if case .staggeredClaims = mode {
                let canClaim = concurrentClaimCount == 0
                    || (concurrentClaimCount == 1 && staggeredClaimAvailable)
                guard canClaim, concurrentClaimCount < 2 else {
                    return try json(["type": "tool_claim", "claim": NSNull()])
                }
                concurrentClaimCount += 1
                return try json([
                    "type": "tool_claim",
                    "claim": [
                        "worker_id": "worker-1",
                        "claim_token": "claim-token-\(concurrentClaimCount)",
                        "invocation": invocation(
                            status: "running",
                            version: 2,
                            index: concurrentClaimCount
                        ),
                    ],
                ])
            }
            let hasClaim = switch mode {
            case .oneSideEffectingClaim, .preflightDeniedClaim, .renewingClaim,
                 .renewingClaimAfterTransientFailures: true
            case .idle, .onePendingApproval, .terminalEvent: false
            case .eventDrivenClaim: eventDelivered
            case .concurrentClaims, .staggeredClaims: false
            case let .delayedClaim(emptyAttempts): claimAttempts > emptyAttempts
            }
            guard hasClaim, !didClaim else {
                return try json(["type": "tool_claim", "claim": NSNull()])
            }
            didClaim = true
            var claim: [String: Any] = [
                "worker_id": "worker-1",
                "claim_token": "claim-token-1",
                "invocation": invocation(status: "running", version: 2),
            ]
            if case .preflightDeniedClaim = mode {
                claim["preflight_outcome"] = [
                    "type": "failed",
                    "error": "skill_activation_required",
                    "detail": [
                        "required_skill_refs": ["product-skill:test"],
                    ],
                ]
            }
            return try json([
                "type": "tool_claim",
                "claim": claim,
            ])
        case "renew_tool_claim":
            renewals += 1
            if case let .renewingClaimAfterTransientFailures(count) = mode,
               renewals <= count {
                throw URLError(.networkConnectionLost)
            }
            let accepted = switch mode {
            case let .renewingClaim(renewalAccepted): renewalAccepted
            case .renewingClaimAfterTransientFailures: true
            case .idle, .oneSideEffectingClaim, .preflightDeniedClaim,
                 .onePendingApproval, .eventDrivenClaim, .terminalEvent, .concurrentClaims,
                 .staggeredClaims, .delayedClaim: true
            }
            return try json([
                "type": "tool_claim_renewed",
                "renewed": accepted,
            ])
        case "commit_tool":
            commits += 1
            return try json([
                "type": "tool_commit",
                "result": [
                    "invocation": invocation(status: "needs_review", version: 3),
                    "run": run(),
                ],
            ])
        case "list_pending_tool_approvals":
            guard case .onePendingApproval = mode else {
                return try json(["type": "pending_tool_approvals", "invocations": []])
            }
            return try json([
                "type": "pending_tool_approvals",
                "invocations": [approvalInvocation(status: "pending", version: 2)],
            ])
        case "decide_tool_approval":
            return try json([
                "type": "tool_approval",
                "result": [
                    "invocation": approvalInvocation(status: "pending", version: 3, approved: true),
                    "run": run(status: "running"),
                ],
            ])
        default:
            throw CocoaError(.featureUnsupported)
        }
    }

    func lastCommand() throws -> [String: LocalAgentJSONValue] {
        guard let data = commands.last else { throw CocoaError(.fileNoSuchFile) }
        let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
        guard case let .object(object) = value else { throw CocoaError(.fileReadCorruptFile) }
        return object
    }

    func committedOutcomeType() throws -> LocalAgentJSONValue? {
        for data in commands.reversed() {
            let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
            guard case let .object(object) = value,
                  object["type"] == .string("commit_tool"),
                  case let .object(outcome)? = object["outcome"] else { continue }
            return outcome["type"]
        }
        return nil
    }

    func renewalCount() -> Int { renewals }

    func commitCount() -> Int { commits }

    func claimAttemptCount() -> Int { claimAttempts }

    func eventListRequestCount() -> Int { eventListRequests }

    func makeStaggeredClaimAvailable() {
        staggeredClaimAvailable = true
    }

    private var supportsEventStream: Bool {
        switch mode {
        case .eventDrivenClaim, .terminalEvent,
             .oneSideEffectingClaim, .preflightDeniedClaim: true
        case .idle, .onePendingApproval, .renewingClaim,
             .renewingClaimAfterTransientFailures, .concurrentClaims,
             .staggeredClaims, .delayedClaim: false
        }
    }

    private func invocation(status: String, version: Int, index: Int = 1) -> [String: Any] {
        [
            "invocation_id": "invocation-\(index)",
            "run_id": "run-\(index)",
            "batch_id": "batch-\(index)",
            "call_id": "call-\(index)",
            "tool_name": "write_local_state",
            "execution_route": "platform",
            "arguments": [:],
            "side_effecting": true,
            "requires_approval": false,
            "approval_status": "not_required",
            "status": status,
            "version": version,
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func approvalInvocation(
        status: String,
        version: Int,
        approved: Bool = false
    ) -> [String: Any] {
        [
            "invocation_id": "approval-invocation-1",
            "run_id": "run-1",
            "batch_id": "batch-1",
            "call_id": "call-1",
            "tool_name": "commit_edit_session",
            "execution_route": "platform",
            "arguments": ["session_id": "session-1"],
            "side_effecting": true,
            "requires_approval": true,
            "approval_status": approved ? "approved" : "pending",
            "approval_decided_by": approved ? "macos-local-approval" : NSNull(),
            "approval_reason": approved ? "Approved in the local client." : NSNull(),
            "approval_decided_at_unix_ms": approved ? 3 : NSNull(),
            "status": status,
            "version": version,
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func run(status: String = "needs_review") -> [String: Any] {
        [
            "run_id": "run-1",
            "owner_user_id": "user-1",
            "owner_entity_type": "conversation_turn",
            "owner_entity_id": "turn-1",
            "profile_key": "main_chat",
            "model_config_ref": "model-1",
            "model_config_revision": "revision-1",
            "capability_policy_revision": "policy-1",
            "input": [:],
            "status": status,
            "iteration": 1,
            "model_attempt": 1,
            "max_iterations": 8,
            "version": 3,
            "checkpoint": NSNull(),
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func json(_ object: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: object)
    }
}
