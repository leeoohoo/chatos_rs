import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeConnectorStateStoreTests {
    @Test
    func localApprovalRuntimeLoadsWithoutGatewayPairing() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let stateURL = directory.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.approvalMode = .autoApproval
        state.commandApprovalModelConfigID = "approval-model"
        state.commandApprovalThinkingLevel = "medium"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let credentialStore = NativeLocalAgentModelCredentialStore(
            rootURL: directory
                .appendingPathComponent("Secrets", isDirectory: true)
                .appendingPathComponent("LocalAgentModels", isDirectory: true)
        )
        try credentialStore.saveWithoutUserInteraction(
            "local-api-key",
            ownerUserID: "owner-1",
            modelConfigRef: "approval-model"
        )
        let snapshot = LocalAgentModelConfigSnapshot(
            ownerUserID: "owner-1",
            modelConfigRef: "approval-model",
            modelConfigRevision: "revision-1",
            credentialRef: "env:CHATOS_LOCAL_AGENT_MODEL_APPROVAL_MODEL",
            baseURL: "https://models.example.test/v1",
            model: "approval-model-name",
            provider: "openai",
            supportsResponses: true,
            temperature: 0,
            maxOutputTokens: 1_200
        )
        let capability = LocalAgentCapabilityPolicySnapshot(
            ownerUserID: "owner-1",
            profileKey: "main_chat",
            capabilityPolicyRevision: "policy-1"
        )
        let bootstrap = NativeLocalAgentBootstrapResult(
            modelSnapshots: [snapshot],
            modelOptions: [],
            defaultConversationModelConfigRef: snapshot.modelConfigRef,
            capabilitySnapshot: capability,
            capabilitySnapshotsByModelConfigRef: [snapshot.modelConfigRef: capability],
            pluginInstallerCapabilitySnapshotsByModelConfigRef: [:]
        )
        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.example.test")!,
                stateURL: stateURL
            ),
            ticketProvider: RejectingTicketProvider()
        )

        await service.configureLocalApprovalAgent(
            ownerUserID: "owner-1",
            bootstrap: bootstrap
        )

        let runtime = await service.localApprovalAgentRuntime
        #expect(runtime?.ownerUserID == "owner-1")
        #expect(runtime?.model.configID == "approval-model")
        #expect(runtime?.model.apiKey == "local-api-key")
        #expect(runtime?.systemPrompt == LocalAgentPromptCatalog.render(.approvalSystem))
        #expect(runtime?.thinkingLevel == "medium")
        let status = try await service.fetchStatus()
        #expect(!status.configured)
    }

    @Test
    func unavailableLocalApprovalRuntimeNeverReportsGatewayPairingAsTheCause() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let stateURL = directory.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.approvalMode = .autoApproval
        state.commandApprovalModelConfigID = "approval-model"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.example.test")!,
                stateURL: stateURL
            ),
            ticketProvider: RejectingTicketProvider()
        )
        let root = directory.appendingPathComponent("project", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let decisionTask = Task {
            await service.approvalDecision(
                requestID: "request-1",
                command: "/bin/zsh",
                arguments: ["-lc", "pwd"],
                cwd: root,
                projectRoot: root,
                source: "Local Agent Task",
                risk: .init(level: "low", reason: nil),
                purpose: "确认工作目录",
                workspaceID: "workspace-1"
            )
        }
        var pending: [LocalConnectorPendingApproval] = []
        for _ in 0..<100 {
            pending = try await service.fetchPendingApprovals()
            if !pending.isEmpty { break }
            try await Task.sleep(for: .milliseconds(10))
        }
        let approval = try #require(pending.first)
        #expect(approval.reason == "本机审批 Agent 尚未从本地任务运行时载入。")
        #expect(approval.reason?.contains("网关配对") == false)

        try await service.resolveApproval(id: approval.id, decision: .decline)
        let decision = await decisionTask.value
        #expect(decision == .deny(reason: "用户已在本机拒绝。"))
    }

    @Test
    func declaredPluginApprovalScopeIgnoresTaskLabelButExactCommandScopeDoesNot() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let stateURL = directory.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "owner-1", username: "owner", displayName: "Owner", role: "user")
        state.deviceID = "device-1"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.example.test")!,
                stateURL: stateURL
            ),
            ticketProvider: RejectingTicketProvider()
        )
        let root = directory.appendingPathComponent("project", isDirectory: true)
        let common = "仅暴露任务新建页和用户主动共享页；browser.chrome.attach"

        let declaredA = await service.approvalSessionScope(
            base: "agent-plugin:browser-cdp-mcp:browser_session_open:browser.chrome.attach",
            command: "agent_plugin:browser-cdp-mcp · browser_session_open",
            arguments: ["任务 A"], cwd: root, projectRoot: root, workspaceID: "workspace-1",
            requestedPermissionsDescription: common, reuse: .declaredScope
        )
        let declaredB = await service.approvalSessionScope(
            base: "agent-plugin:browser-cdp-mcp:browser_session_open:browser.chrome.attach",
            command: "agent_plugin:browser-cdp-mcp · browser_session_open",
            arguments: ["任务 B"], cwd: root, projectRoot: root, workspaceID: "workspace-1",
            requestedPermissionsDescription: common, reuse: .declaredScope
        )
        let exactA = await service.approvalSessionScope(
            base: "local-agent-terminal:conversation-1", command: "/bin/zsh",
            arguments: ["-lc", "echo A"], cwd: root, projectRoot: root,
            workspaceID: "workspace-1", requestedPermissionsDescription: "项目命令",
            reuse: .exactRequest
        )
        let exactB = await service.approvalSessionScope(
            base: "local-agent-terminal:conversation-1", command: "/bin/zsh",
            arguments: ["-lc", "echo B"], cwd: root, projectRoot: root,
            workspaceID: "workspace-1", requestedPermissionsDescription: "项目命令",
            reuse: .exactRequest
        )
        let exactDifferentPurpose = await service.approvalSessionScope(
            base: "local-agent-terminal:conversation-1", command: "/bin/zsh",
            arguments: ["-lc", "echo A"], cwd: root, projectRoot: root,
            workspaceID: "workspace-1", purpose: "执行另一项检查",
            requestedPermissionsDescription: "项目命令", reuse: .exactRequest
        )

        #expect(declaredA == declaredB)
        #expect(exactA != exactB)
        #expect(exactA != exactDifferentPurpose)
    }

    @Test
    func pluginUpdateStateUsesInstalledReleaseVersionAndArtifact() {
        let installed = NativeInstalledPluginRecord(
            pluginID: "plugin-a",
            releaseID: "release-1",
            version: "0.1.2",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: "/tmp/plugin-a/0.1.2",
            installedAt: "2026-09-04T00:00:00Z"
        )
        #expect(!NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "release-1",
                version: "0.1.2",
                artifactSHA256: String(repeating: "a", count: 64),
                npmPackage: nil
            )
        ))
        #expect(!NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "release-republished",
                version: "0.1.2",
                artifactSHA256: String(repeating: "a", count: 64),
                npmPackage: nil
            )
        ))
        #expect(NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "release-2",
                version: "0.1.3",
                artifactSHA256: String(repeating: "b", count: 64),
                npmPackage: nil
            )
        ))
        #expect(!NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "release-missing-version",
                version: nil,
                artifactSHA256: String(repeating: "b", count: 64),
                npmPackage: nil
            )
        ))
    }

    @Test
    func webDesignStudioUpdateFrom0100To0110IsAvailable() {
        let installed = NativeInstalledPluginRecord(
            pluginID: "14993149-32e7-40a8-b166-dc7639edebfc",
            releaseID: "9a8dbb08-bb45-434e-a59e-40550867e7fb",
            version: "0.10.0",
            artifactSHA256: "e22f2a4e5b14d3e434fa4b01541d74dbcef31de0fc2586083962549467c0e20a",
            installationPath: "/tmp/chatos-web-design-studio/0.10.0",
            installedAt: "2026-09-07T00:00:00Z"
        )

        #expect(NativeLocalConnectorService.pluginUpdateAvailable(
            installed: installed,
            release: .init(
                id: "c8a8a26c-4605-4477-acb0-9a84a04b3098",
                version: "0.11.0",
                artifactSHA256: "75de6a325efb6bc1c3034ba9e207461d7dcfb6945498fa3c08d27e82d4a673c3",
                npmPackage: nil
            )
        ))
    }

    @Test
    func dynamicGatewayRequestsBypassCachedMarketplaceResponses() {
        let request = NativeConnectorGateway.dynamicRequest(
            url: URL(string: "https://connector.jgoool.com/api/plugin-management/plugins/install-sources")!
        )

        #expect(request.cachePolicy == .reloadIgnoringLocalAndRemoteCacheData)
        #expect(request.value(forHTTPHeaderField: "Cache-Control") == "no-cache, no-store")
        #expect(request.value(forHTTPHeaderField: "Pragma") == "no-cache")
    }

    @Test
    func stateRoundTripsWithoutLosingSecurityOrPluginSettings() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = NativeConnectorStateStore(stateURL: directory.appendingPathComponent("state.json"))
        var state = NativeConnectorPersistentState.empty
        state.deploymentIdentifier = "production"
        state.gatewayBaseURL = "https://connector.jgoool.com"
        state.deviceID = "device-1"
        state.deviceName = "Test Mac"
        state.gatewayConnectionEnabled = false
        state.developerMode = true
        state.sandboxEnabled = false
        state.permissionProfileID = ":read-only"
        state.networkAccess = "restricted"
        state.installedPluginIDs = ["plugin-a"]
        state.commandApprovalModelConfigID = "approval-model"
        state.commandApprovalThinkingLevel = "high"
        state.installedPluginRecords = [
            "plugin-a": .init(
                pluginID: "plugin-a",
                releaseID: "release-a",
                version: "1.2.3",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: "/tmp/plugin-a/1.2.3",
                installedAt: "2026-08-25T00:00:00Z",
                pluginKey: "plugin-a@official"
            ),
        ]
        state.pluginPreferences = ["plugin-a": false]
        state.localAgentCapabilityRevisionsByModelConfigID = [
            "model-gpt": "managed-prompt-revision-7",
        ]
        state.localAgentPromptBundleVersion = 7
        state.localAgentTaskSkillChecksum = "sha256:task-skill-7"
        state.workspaces = [
            .init(id: "workspace-1", alias: "Project", absoluteRoot: "/tmp/project", fingerprint: "abc")
        ]

        try store.save(state)
        let restored = try store.load()

        #expect(restored.deploymentIdentifier == "production")
        #expect(restored.gatewayBaseURL == "https://connector.jgoool.com")
        #expect(restored.deviceID == "device-1")
        #expect(restored.deviceName == "Test Mac")
        #expect(restored.gatewayConnectionEnabled == false)
        #expect(restored.developerMode)
        #expect(!restored.sandboxEnabled)
        #expect(restored.permissionProfileID == ":read-only")
        #expect(restored.networkAccess == "restricted")
        #expect(restored.installedPluginIDs == ["plugin-a"])
        #expect(restored.commandApprovalModelConfigID == "approval-model")
        #expect(restored.commandApprovalThinkingLevel == "high")
        #expect(restored.installedPluginRecords?["plugin-a"]?.version == "1.2.3")
        #expect(restored.installedPluginRecords?["plugin-a"]?.pluginKey == "plugin-a@official")
        #expect(restored.pluginPreferences["plugin-a"] == false)
        #expect(
            restored.localAgentCapabilityRevisionsByModelConfigID?["model-gpt"]
                == "managed-prompt-revision-7"
        )
        #expect(restored.localAgentPromptBundleVersion == 7)
        #expect(restored.localAgentTaskSkillChecksum == "sha256:task-skill-7")
        #expect(restored.workspaces.first?.absoluteRoot == "/tmp/project")
    }

    @Test
    func connectorPairingIsScopedToOneDeployment() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let stateURL = directory.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.deploymentIdentifier = "local"
        state.gatewayBaseURL = "http://127.0.0.1:9080/api/connector"
        state.deviceID = "device-local"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)

        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.jgoool.com")!,
                stateURL: stateURL,
                deploymentIdentifier: "production"
            ),
            ticketProvider: RejectingTicketProvider()
        )

        let matches = await service.pairingMatchesCurrentDeployment
        #expect(!matches)
    }

    @Test
    func loginCanRenewOnlyTheMatchingAccountsExistingDevice() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let stateURL = directory.appendingPathComponent("state.json")
        var state = NativeConnectorPersistentState.empty
        state.deploymentIdentifier = "production"
        state.gatewayBaseURL = "https://connector.jgoool.com"
        state.deviceID = "device-existing"
        state.user = .init(id: "alice", username: "alice", displayName: "Alice", role: "user")
        try NativeConnectorStateStore(stateURL: stateURL).save(state)

        let service = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "https://connector.jgoool.com")!,
                stateURL: stateURL,
                deploymentIdentifier: "production"
            ),
            ticketProvider: RejectingTicketProvider()
        )

        #expect(await service.canReuseExistingPairing(ownerUserID: "alice"))
        #expect(!(await service.canReuseExistingPairing(ownerUserID: "bob")))
    }

    @Test
    func missingOrForbiddenServerDeviceIsRecreatedButOtherFailuresAreNot() {
        #expect(NativeLocalConnectorService.deviceMustBeRecreated(
            after: .server(status: 404, message: "missing")
        ))
        #expect(NativeLocalConnectorService.deviceMustBeRecreated(
            after: .server(status: 403, message: "forbidden")
        ))
        #expect(!NativeLocalConnectorService.deviceMustBeRecreated(
            after: .server(status: 500, message: "offline")
        ))
        #expect(!NativeLocalConnectorService.deviceMustBeRecreated(after: .notPaired))
    }

    @Test
    func stalePluginIdentityMigratesToRepublishedCatalogEntryByArtifact() throws {
        var state = NativeConnectorPersistentState.empty
        state.installedPluginIDs = ["old-plugin-id"]
        state.pluginPreferences = ["old-plugin-id": false]
        state.installedPluginRecords = [
            "old-plugin-id": .init(
                pluginID: "old-plugin-id",
                releaseID: "old-release-id",
                version: "0.1.6",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: "/tmp/browser/0.1.6",
                installedAt: "2026-08-25T00:00:00Z"
            ),
        ]
        let source = GatewayPluginSourceDTO(
            catalog: GatewayPluginCatalogDTO(
                id: "current-plugin-id",
                displayName: "Browser CDP",
                name: "chatos-browser-cdp",
                description: nil,
                publisher: nil,
                interface: nil
            ),
            release: GatewayPluginReleaseDTO(
                id: "current-release-id",
                version: "0.1.6",
                artifactSHA256: String(repeating: "a", count: 64),
                npmPackage: nil
            ),
            preference: nil
        )

        let changed = NativeLocalConnectorService.reconcileInstalledPluginIdentities(
            state: &state,
            sources: [source]
        )

        #expect(changed)
        #expect(state.installedPluginIDs == ["current-plugin-id"])
        #expect(state.installedPluginRecords?["old-plugin-id"] == nil)
        #expect(state.installedPluginRecords?["current-plugin-id"]?.pluginID == "current-plugin-id")
        #expect(state.installedPluginRecords?["current-plugin-id"]?.releaseID == "current-release-id")
        #expect(state.pluginPreferences["old-plugin-id"] == nil)
        #expect(state.pluginPreferences["current-plugin-id"] == false)
    }

    @Test
    func stablePluginKeySelectsTheCorrectCatalogEntryWhenArtifactsCollide() throws {
        var state = NativeConnectorPersistentState.empty
        state.installedPluginIDs = ["old-computer-use-id"]
        state.installedPluginRecords = [
            "old-computer-use-id": .init(
                pluginID: "old-computer-use-id",
                releaseID: "old-release-id",
                version: "0.8.11",
                artifactSHA256: String(repeating: "c", count: 64),
                installationPath: "/tmp/computer-use/0.8.11",
                installedAt: "2026-08-25T00:00:00Z",
                pluginKey: "open-computer-use@chatos-marketplace"
            ),
        ]
        let sources = [
            GatewayPluginSourceDTO(
                catalog: GatewayPluginCatalogDTO(
                    id: "unrelated-plugin-id",
                    displayName: "Unrelated",
                    name: "unrelated",
                    description: nil,
                    publisher: nil,
                    interface: nil,
                    pluginKey: "unrelated@chatos-marketplace"
                ),
                release: GatewayPluginReleaseDTO(
                    id: "unrelated-release-id",
                    version: "0.8.11",
                    artifactSHA256: String(repeating: "c", count: 64),
                    npmPackage: nil
                ),
                preference: nil
            ),
            GatewayPluginSourceDTO(
                catalog: GatewayPluginCatalogDTO(
                    id: "current-computer-use-id",
                    displayName: "Computer Use",
                    name: "open-computer-use",
                    description: nil,
                    publisher: nil,
                    interface: nil,
                    pluginKey: "open-computer-use@chatos-marketplace"
                ),
                release: GatewayPluginReleaseDTO(
                    id: "current-release-id",
                    version: "0.8.11",
                    artifactSHA256: String(repeating: "c", count: 64),
                    npmPackage: nil
                ),
                preference: nil
            ),
        ]

        let changed = NativeLocalConnectorService.reconcileInstalledPluginIdentities(
            state: &state,
            sources: sources
        )

        #expect(changed)
        #expect(state.installedPluginIDs == ["current-computer-use-id"])
        #expect(state.installedPluginRecords?["current-computer-use-id"]?.releaseID == "current-release-id")
        #expect(
            state.installedPluginRecords?["current-computer-use-id"]?.pluginKey
                == "open-computer-use@chatos-marketplace"
        )
    }

    @Test
    func currentInstallRemovesLegacyDuplicateByInstalledPackageName() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let legacyInstall = directory.appendingPathComponent("0.8.11", isDirectory: true)
        try FileManager.default.createDirectory(at: legacyInstall, withIntermediateDirectories: true)
        let legacyManifest = """
        {
          "schemaVersion": 3,
          "name": "open-computer-use",
          "version": "0.8.11",
          "mcpServers": {},
          "skills": []
        }
        """
        try Data(legacyManifest.utf8).write(
            to: legacyInstall.appendingPathComponent("chatos.plugin.json")
        )

        var state = NativeConnectorPersistentState.empty
        state.installedPluginIDs = ["legacy-id", "current-id"]
        state.pluginPreferences = ["legacy-id": false, "current-id": true]
        state.installedPluginRecords = [
            "legacy-id": .init(
                pluginID: "legacy-id",
                releaseID: "legacy-release",
                version: "0.8.11",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: legacyInstall.path,
                installedAt: "2026-08-25T00:00:00Z"
            ),
            "current-id": .init(
                pluginID: "current-id",
                releaseID: "current-release",
                version: "0.8.12",
                artifactSHA256: String(repeating: "b", count: 64),
                installationPath: "/tmp/computer-use/0.8.12",
                installedAt: "2026-09-01T00:00:00Z",
                pluginKey: "open-computer-use@chatos-marketplace"
            ),
        ]
        let source = GatewayPluginSourceDTO(
            catalog: GatewayPluginCatalogDTO(
                id: "current-id",
                displayName: "Visual Computer Use",
                name: "open-computer-use",
                description: nil,
                publisher: nil,
                interface: nil,
                pluginKey: "open-computer-use@chatos-marketplace"
            ),
            release: GatewayPluginReleaseDTO(
                id: "current-release",
                version: "0.8.12",
                artifactSHA256: String(repeating: "b", count: 64),
                npmPackage: nil
            ),
            preference: nil
        )

        let changed = NativeLocalConnectorService.reconcileInstalledPluginIdentities(
            state: &state,
            sources: [source]
        )

        #expect(changed)
        #expect(state.installedPluginIDs == ["current-id"])
        #expect(state.installedPluginRecords?["legacy-id"] == nil)
        #expect(state.installedPluginRecords?["current-id"]?.version == "0.8.12")
        #expect(state.pluginPreferences["legacy-id"] == nil)
        #expect(state.pluginPreferences["current-id"] == true)
    }

    @Test
    func activityHistoryRoundTripsIndependentlyFromConnectorState() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = NativeConnectorActivityHistoryStore(
            historyURL: directory.appendingPathComponent("activity-history.json")
        )
        let history = NativeConnectorActivityHistory(
            approvals: [.init(
                id: "approval-1", command: "pwd", cwd: "/tmp", source: "test",
                mode: .requestApproval, decision: "accept", risk: "low",
                reason: nil, createdAt: "2026-10-19T00:00:00Z"
            )],
            commands: [.init(
                id: "command-1", source: "test", workspaceAlias: "Project", cwd: "/tmp",
                display: "pwd", status: "completed", exitCode: 0, stdoutPreview: "/tmp",
                stderrPreview: nil, error: nil, startedAt: "2026-10-19T00:00:00Z"
            )]
        )

        try store.save(history)

        #expect(try store.load() == history)
    }
}

private struct RejectingTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String {
        throw URLError(.notConnectedToInternet)
    }
}
