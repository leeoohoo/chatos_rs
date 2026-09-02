@testable import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native Plugin Runtime")
struct NativePluginRuntimeTests {
    @Test("local HTTP Plugin applications launch from package.json.bin and become reachable")
    func localHTTPPluginApplicationLaunches() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let installation = root.appendingPathComponent("plugin", isDirectory: true)
        let binDirectory = installation.appendingPathComponent("bin", isDirectory: true)
        let uiDirectory = installation.appendingPathComponent("ui", isDirectory: true)
        let runtimeRoot = root.appendingPathComponent("runtime", isDirectory: true)
        try FileManager.default.createDirectory(at: binDirectory, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: uiDirectory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try Data(#"{"name":"demo-app","version":"1.0.0","bin":{"demo-app":"bin/demo-app"}}"#.utf8)
            .write(to: installation.appendingPathComponent("package.json"))
        try Data("<html><body>Plugin application ready</body></html>".utf8)
            .write(to: uiDirectory.appendingPathComponent("index.html"))
        let launcher = binDirectory.appendingPathComponent("demo-app")
        let script = #"""
        #!/bin/sh
        exec node -e 'const http=require("http");const port=Number(process.env.CHATOS_PLUGIN_APP_PORT);http.createServer((req,res)=>{res.writeHead(200,{"content-type":"text/html"});res.end("Plugin application ready")}).listen(port,"127.0.0.1")'
        """#
        try Data(script.utf8).write(to: launcher)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: launcher.path)

        let manifestData = Data(#"""
        {
          "schemaVersion": 3,
          "name": "demo-app",
          "version": "1.0.0",
          "description": "Demo application",
          "ui": [{
            "componentKey": "workbench",
            "source": "./ui/index.html",
            "title": "Demo",
            "surface": "workbench",
            "runtime": {
              "type": "local_http",
              "bin": "demo-app",
              "healthPath": "/",
              "launchTimeoutMs": 5000
            }
          }],
          "permissions": [{
            "permission": "process.spawn",
            "required": true,
            "components": ["workbench"]
          }]
        }
        """#.utf8)
        let manifest = try JSONDecoder().decode(NativePluginManifest.self, from: manifestData)
        let record = NativeInstalledPluginRecord(
            pluginID: "plugin-demo",
            releaseID: "release-demo",
            version: "1.0.0",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: installation.path,
            installedAt: "2026-09-02T00:00:00Z"
        )
        let application = LocalConnectorPluginApplication(
            pluginID: record.pluginID,
            componentKey: "workbench",
            displayName: "Demo",
            description: "Demo application",
            requiresLocalRuntime: true
        )
        let runtime = NativePluginApplicationRuntime()
        defer { Task { await runtime.stopAll() } }

        let launch = try await runtime.launch(
            record: record,
            manifest: manifest,
            contribution: manifest.ui[0],
            runtimeRootURL: runtimeRoot,
            application: application,
            hostContext: .init(
                ownerUserID: "user-1",
                deviceID: "device-1",
                workspaceID: nil,
                workspaceRoot: nil,
                projectID: nil,
                projectName: nil
            )
        )
        let body = try await URLSession.shared.data(from: launch.url).0
        #expect(String(decoding: body, as: UTF8.self) == "Plugin application ready")
        #expect(FileManager.default.fileExists(
            atPath: runtimeRoot.appendingPathComponent("data", isDirectory: true).path
        ))
        await runtime.stopAll()
    }

    @Test("all plugins use different data directories for different ChatOS users")
    func allPluginsAreUserIsolatedWithoutRuntimeContext() throws {
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data(#"{"schemaVersion":3,"name":"fixture","version":"1.0.0"}"#.utf8)
        )
        let runtimeRoot = URL(fileURLWithPath: "/tmp/chatos-plugin-runtime-tests", isDirectory: true)
        let first = try NativePluginRuntimeContextResolver.resolve(
            manifest: manifest,
            componentKey: "fixture",
            runtimeRootURL: runtimeRoot,
            pluginID: "plugin-1",
            host: .init(
                ownerUserID: "user-1",
                deviceID: "device-1",
                workspaceID: nil,
                workspaceRoot: nil,
                projectID: nil,
                projectName: nil
            )
        )
        let second = try NativePluginRuntimeContextResolver.resolve(
            manifest: manifest,
            componentKey: "fixture",
            runtimeRootURL: runtimeRoot,
            pluginID: "plugin-1",
            host: .init(
                ownerUserID: "user-2",
                deviceID: "device-1",
                workspaceID: nil,
                workspaceRoot: nil,
                projectID: nil,
                projectName: nil
            )
        )

        #expect(first.dataURL != second.dataURL)
        #expect(first.cacheURL != second.cacheURL)
        #expect(first.dataURL.path.contains("/data/users/"))
        #expect(second.dataURL.path.contains("/data/users/"))
        #expect(
            first.websiteDataStoreID(applicationID: "plugin-1:fixture")
                != second.websiteDataStoreID(applicationID: "plugin-1:fixture")
        )
    }

    @Test("project plugins share one scope between MCP and UI and fall back to a device public project")
    func projectRuntimeContextMatchesMCPAndUI() throws {
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data(#"""
            {
              "schemaVersion": 3,
              "name": "fixture",
              "version": "1.0.0",
              "mcpServers": {"fixture-mcp": {"type": "stdio", "bin": "fixture"}},
              "ui": [{"componentKey": "fixture-ui", "source": "./ui/index.html"}],
              "runtimeContext": {
                "scope": "project",
                "components": ["fixture-mcp", "fixture-ui"],
                "optional": ["project.id", "workspace.id", "workspace.root"],
                "storageIsolation": "project",
                "missingContext": "device"
              }
            }
            """#.utf8)
        )
        let runtimeRoot = URL(fileURLWithPath: "/tmp/chatos-plugin-runtime-tests", isDirectory: true)
        let projectHost = NativePluginHostContext(
            ownerUserID: "user-1",
            deviceID: "device-1",
            workspaceID: "workspace-1",
            workspaceRoot: URL(fileURLWithPath: "/tmp/workspace", isDirectory: true),
            projectID: "project-1",
            projectName: "Project One"
        )
        let mcp = try NativePluginRuntimeContextResolver.resolve(
            manifest: manifest,
            componentKey: "fixture-mcp",
            runtimeRootURL: runtimeRoot,
            pluginID: "plugin-1",
            host: projectHost
        )
        let ui = try NativePluginRuntimeContextResolver.resolve(
            manifest: manifest,
            componentKey: "fixture-ui",
            runtimeRootURL: runtimeRoot,
            pluginID: "plugin-1",
            host: projectHost
        )
        let otherProject = try NativePluginRuntimeContextResolver.resolve(
            manifest: manifest,
            componentKey: "fixture-mcp",
            runtimeRootURL: runtimeRoot,
            pluginID: "plugin-1",
            host: .init(
                ownerUserID: "user-1",
                deviceID: "device-1",
                workspaceID: "workspace-1",
                workspaceRoot: projectHost.workspaceRoot,
                projectID: "project-2",
                projectName: "Project Two"
            )
        )
        let publicProject = try NativePluginRuntimeContextResolver.resolve(
            manifest: manifest,
            componentKey: "fixture-mcp",
            runtimeRootURL: runtimeRoot,
            pluginID: "plugin-1",
            host: .init(
                ownerUserID: "user-1",
                deviceID: "device-1",
                workspaceID: nil,
                workspaceRoot: nil,
                projectID: nil,
                projectName: nil
            )
        )

        #expect(mcp.dataURL == ui.dataURL)
        #expect(mcp.cacheURL == ui.cacheURL)
        #expect(
            mcp.websiteDataStoreID(applicationID: "plugin-1:fixture-ui")
                == ui.websiteDataStoreID(applicationID: "plugin-1:fixture-ui")
        )
        #expect(mcp.dataURL != otherProject.dataURL)
        #expect(
            mcp.websiteDataStoreID(applicationID: "plugin-1:fixture-ui")
                != otherProject.websiteDataStoreID(applicationID: "plugin-1:fixture-ui")
        )
        #expect(mcp.environment["CHATOS_CONTEXT_SCOPE"] == "project")
        #expect(mcp.environment["CHATOS_PROJECT_ID"] == "project-1")
        #expect(publicProject.environment["CHATOS_CONTEXT_SCOPE"] == "device")
        #expect(publicProject.environment["CHATOS_PROJECT_ID"] == nil)
        #expect(publicProject.dataURL != mcp.dataURL)
    }

    @Test("plugin project root resolves the current project beneath a broad connector workspace")
    func pluginProjectRootUsesCurrentProjectInsteadOfConnectorRoot() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let project = root.appendingPathComponent("projects/space-station", isDirectory: true)
        try FileManager.default.createDirectory(at: project, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let workspace = LocalConnectorWorkspace(
            id: "workspace-1",
            alias: "broad-workspace",
            absoluteRoot: root.path,
            fingerprint: "fixture"
        )

        let resolved = try NativePluginProjectRootResolver.resolve(
            rawPath: "projects/space-station",
            workspace: workspace
        )

        #expect(resolved.path == project.resolvingSymlinksInPath().path)
        #expect(resolved.path != root.resolvingSymlinksInPath().path)
    }

    @Test("plugin project root rejects paths outside the authorized connector workspace")
    func pluginProjectRootRejectsEscape() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let workspace = LocalConnectorWorkspace(
            id: "workspace-1",
            alias: "workspace",
            absoluteRoot: root.path,
            fingerprint: "fixture"
        )

        #expect(throws: NativePluginRuntimeError.self) {
            try NativePluginProjectRootResolver.resolve(
                rawPath: FileManager.default.homeDirectoryForCurrentUser.path,
                workspace: workspace
            )
        }
    }

    @Test("device-only plugin relay accepts no workspace and rejects workspace permissions")
    func deviceOnlyPluginRelayScope() throws {
        let scope = try NativePluginRelayScope.resolve(workspaceID: "", workspaces: [])

        #expect(scope.workspaceID == nil)
        try scope.validate(permissionSnapshot: ["process.spawn", "browser.page.read"])
        #expect(throws: NativePluginRuntimeError.self) {
            try scope.validate(permissionSnapshot: ["process.spawn", "workspace.read"])
        }
    }

    @Test("plugin relay rejects a workspace that is not registered on this device")
    func pluginRelayRejectsUnknownWorkspace() {
        #expect(throws: NativePluginRuntimeError.self) {
            try NativePluginRelayScope.resolve(workspaceID: "workspace-other", workspaces: [])
        }
    }

    @Test
    func browserSessionApprovalSummaryExplainsIsolationInsteadOfOnlyHashingArguments() {
        let summary = NativeLocalConnectorService.safeArgumentSummary(
            toolName: "browser_session_open",
            arguments: .object([
                "mode": .string("managed"),
                "headless": .bool(true),
                "persistent_profile": .bool(false),
            ])
        )

        #expect(summary.contains("隔离浏览器会话"))
        #expect(summary.contains("模式 managed"))
        #expect(summary.contains("Headless 是"))
        #expect(summary.contains("持久化浏览器资料 否"))
        #expect(!summary.contains("内容摘要"))
    }

    @Test
    func browserSessionOpenInheritsTaskTitleForNativeChromeGroup() {
        let arguments = NativeLocalConnectorService.browserSessionArguments(
            arguments: .object(["mode": .string("chrome_extension")]),
            relayBody: [
                "task_id": .string("task-123"),
                "task_title": .string("WMS 发布验证"),
            ]
        )

        #expect(arguments.jsonObject?["session_name"]?.jsonString == "WMS 发布验证")
    }

    @Test
    func browserSessionOpenPreservesExplicitSessionName() {
        let arguments = NativeLocalConnectorService.browserSessionArguments(
            arguments: .object([
                "mode": .string("chrome_extension"),
                "session_name": .string("Explicit group"),
            ]),
            relayBody: ["task_title": .string("Ignored title")]
        )

        #expect(arguments.jsonObject?["session_name"]?.jsonString == "Explicit group")
    }

    @Test
    func browserSessionPermissionDescriptionStatesExistingChromeIsNotReused() {
        let description = NativeLocalConnectorService.permissionDescription(
            toolName: "browser_session_open",
            requiredPermissions: ["browser.managed.launch"]
        )

        #expect(description.contains("不连接用户现有 Chrome"))
        #expect(description.contains("browser.managed.launch"))
    }

    @Test
    func rawCDPApprovalSummaryIncludesMethodAndReadOnlyExpression() {
        let summary = NativeLocalConnectorService.safeArgumentSummary(
            toolName: "browser_cdp_send",
            arguments: .object([
                "method": .string("Runtime.evaluate"),
                "target": .string("page"),
                "params": .object([
                    "expression": .string("document.body.innerText"),
                    "returnByValue": .bool(true),
                ]),
            ])
        )

        #expect(summary.contains("Runtime.evaluate"))
        #expect(summary.contains("document.body.innerText"))
        #expect(summary.contains("expression, returnByValue"))
    }

    @Test
    func rawCDPApprovalSummaryRedactsCredentialReadingExpressions() {
        let summary = NativeLocalConnectorService.safeArgumentSummary(
            toolName: "browser_cdp_send",
            arguments: .object([
                "method": .string("Runtime.evaluate"),
                "params": .object([
                    "expression": .string("localStorage.getItem('token')"),
                ]),
            ])
        )

        #expect(summary.contains("已隐藏敏感表达式"))
        #expect(!summary.contains("getItem"))
    }

    @Test("installation status reports permissions and ready MCP components")
    func installationStatusSnapshot() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("#!/bin/sh\nexit 0\n".utf8).write(to: bin.appendingPathComponent("fixture"))
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: bin.appendingPathComponent("fixture").path
        )
        let skill = root.appendingPathComponent("skills/fixture-skill", isDirectory: true)
        try FileManager.default.createDirectory(at: skill, withIntermediateDirectories: true)
        try Data("---\nname: fixture-skill\ndescription: Fixture skill.\n---\n".utf8)
            .write(to: skill.appendingPathComponent("SKILL.md"))
        try Data("""
        {
          "schemaVersion": 3,
          "name": "fixture",
          "version": "1.0.0",
          "skills": ["./skills/fixture-skill"],
          "mcpServers": {
            "fixture-mcp": {"type": "stdio", "bin": "fixture", "args": []}
          },
          "permissions": [
            {"permission": "process.spawn", "required": true, "components": ["fixture-mcp"]},
            {"permission": "workspace.read", "required": true, "components": ["fixture-mcp"]}
          ]
        }
        """.utf8).write(to: root.appendingPathComponent("chatos.plugin.json"))

        let item = try NativePluginInstallationStatusBuilder.makeItem(
            record: NativeInstalledPluginRecord(
                pluginID: "plugin-1",
                releaseID: "release-1",
                version: "1.0.0",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: root.path,
                installedAt: "2026-08-26T00:00:00Z"
            ),
            ownerUserID: "user-1",
            deviceID: "device-1",
            platform: "macos-arm64",
            active: true,
            now: Date(timeIntervalSince1970: 0)
        )

        #expect(item.availabilityStatus == "ready")
        #expect(item.permissionStatus == "satisfied")
        #expect(item.grantedPermissions == ["process.spawn", "workspace.read"])
        #expect(item.componentStatuses == [
            GatewayPluginComponentStatus(
                componentKey: "fixture-skill",
                kind: "skill_collection",
                availabilityStatus: "ready",
                lastError: nil,
                lastCheckedAt: "1970-01-01T00:00:00Z"
            ),
            GatewayPluginComponentStatus(
                componentKey: "fixture-mcp",
                kind: "mcp_server",
                availabilityStatus: "ready",
                lastError: nil,
                lastCheckedAt: "1970-01-01T00:00:00Z"
            ),
        ])

        let payload = try JSONEncoder().encode(
            GatewayPluginInstallationStatusMessage(items: [item])
        )
        let object = try #require(JSONSerialization.jsonObject(with: payload) as? [String: Any])
        let items = try #require(object["items"] as? [[String: Any]])
        #expect(items[0]["granted_permissions"] as? [String] == ["process.spawn", "workspace.read"])
        let statuses = try #require(items[0]["component_statuses"] as? [[String: Any]])
        #expect(statuses[0]["kind"] as? String == "skill_collection")
        #expect(statuses[1]["kind"] as? String == "mcp_server")
    }

    @Test("installation status fails closed when a declared Skill is missing")
    func installationStatusMissingSkill() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("#!/bin/sh\nexit 0\n".utf8).write(to: bin.appendingPathComponent("fixture"))
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: bin.appendingPathComponent("fixture").path
        )
        try Data("""
        {
          "schemaVersion": 3,
          "name": "fixture",
          "version": "1.0.0",
          "skills": ["./skills/missing-skill"],
          "mcpServers": {
            "fixture-mcp": {"type": "stdio", "bin": "fixture", "args": []}
          },
          "permissions": []
        }
        """.utf8).write(to: root.appendingPathComponent("chatos.plugin.json"))

        let item = try NativePluginInstallationStatusBuilder.makeItem(
            record: NativeInstalledPluginRecord(
                pluginID: "plugin-1",
                releaseID: "release-1",
                version: "1.0.0",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: root.path,
                installedAt: "2026-08-26T00:00:00Z"
            ),
            ownerUserID: "user-1",
            deviceID: "device-1",
            platform: "macos-arm64",
            active: true
        )

        #expect(item.availabilityStatus == "partially_available")
        #expect(item.componentStatuses.first?.componentKey == "missing-skill")
        #expect(item.componentStatuses.first?.availabilityStatus == "unavailable")
        #expect(item.componentStatuses.first?.lastError == "Plugin Skill 目录不存在")
    }

    @Test("plugin relay prepares a signed Skill component without launching it as an MCP server")
    func pluginSkillPrepareSnapshot() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let skill = root.appendingPathComponent("skills/fixture-skill", isDirectory: true)
        let references = skill.appendingPathComponent("references", isDirectory: true)
        try FileManager.default.createDirectory(at: references, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("""
        ---
        name: fixture-skill
        description: Fixture Skill instructions.
        ---

        # Fixture Skill

        Read [guide.md](references/guide.md) when more detail is needed.
        """.utf8).write(to: skill.appendingPathComponent("SKILL.md"))
        try Data("# Guide\n\nUse fresh screenshots.\n".utf8)
            .write(to: references.appendingPathComponent("guide.md"))
        try Data("""
        {
          "schemaVersion": 3,
          "name": "fixture",
          "version": "1.0.0",
          "skills": ["./skills/fixture-skill"],
          "mcpServers": {}
        }
        """.utf8).write(to: root.appendingPathComponent("chatos.plugin.json"))
        let artifactSHA256 = String(repeating: "a", count: 64)

        let body = try NativePluginSkillSnapshotLoader.prepareBody(
            record: .init(
                pluginID: "plugin-1",
                releaseID: "release-1",
                version: "1.0.0",
                artifactSHA256: artifactSHA256,
                installationPath: root.path,
                installedAt: "2026-08-27T00:00:00Z"
            ),
            componentKey: "fixture-skill",
            skillKeys: ["fixture-skill"],
            expectedContentSHA256: artifactSHA256,
            runID: "run-1",
            adapterSessionID: "adapter-1",
            now: Date(timeIntervalSince1970: 0)
        )

        let object = try body.requireObject()
        #expect(try object.requireString("component_key") == "fixture-skill")
        #expect(try object.requireStringArray("operations") == ["load_skill_resource"])
        #expect(try object.requireString("session_sha256").count == 64)
        let skills = try #require(object["skills"]?.jsonArray)
        #expect(skills.count == 1)
        let snapshot = try skills[0].requireObject()
        #expect(try snapshot.requireString("skill_key") == "fixture-skill")
        #expect(try snapshot.requireString("instructions").contains("Read [guide.md]"))
        #expect(try snapshot.requireString("instructions_sha256").count == 64)
        #expect(try snapshot.requireString("snapshot_sha256").count == 64)
        let resources = try #require(snapshot["resources"]?.jsonArray)
        #expect(resources.count == 1)
        #expect(
            try resources[0].requireObject().requireString("relative_path")
                == "skills/fixture-skill/references/guide.md"
        )
    }

    @Test("installation status fails closed when executable is missing")
    func installationStatusMissingExecutable() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try Data("""
        {
          "schemaVersion": 3,
          "name": "fixture",
          "version": "1.0.0",
          "mcpServers": {
            "fixture-mcp": {"type": "stdio", "bin": "missing", "args": []}
          },
          "permissions": []
        }
        """.utf8).write(to: root.appendingPathComponent("chatos.plugin.json"))

        let item = try NativePluginInstallationStatusBuilder.makeItem(
            record: NativeInstalledPluginRecord(
                pluginID: "plugin-1",
                releaseID: "release-1",
                version: "1.0.0",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: root.path,
                installedAt: "2026-08-26T00:00:00Z"
            ),
            ownerUserID: "user-1",
            deviceID: "device-1",
            platform: "macos-arm64",
            active: true
        )

        #expect(item.availabilityStatus == "unavailable")
        #expect(item.componentStatuses.first?.availabilityStatus == "unavailable")
        #expect(item.lastError == "Plugin 可执行文件不存在")
    }

    @Test("computer use retains its declared launcher for app-agent permission ownership")
    func computerUseRetainsDeclaredLauncher() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let installation = root.appendingPathComponent("plugin", isDirectory: true)
        let launcher = installation
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent("open-computer-use")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(
            at: launcher.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("#!/bin/sh\n".utf8).write(to: launcher)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: launcher.path)
        try Data("""
        {"schemaVersion":3,"name":"open-computer-use","version":"0.3.42","mcpServers":{"computer-use":{"type":"stdio","bin":"open-computer-use","args":["mcp"]}},"permissions":[{"permission":"process.spawn","required":true,"components":["computer-use"]}]}
        """.utf8).write(to: installation.appendingPathComponent("chatos.plugin.json"))

        let launch = try NativePluginManifestLoader.prepare(
            record: .init(
                pluginID: "plugin-1",
                releaseID: "release-1",
                version: "0.3.42",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: installation.path,
                installedAt: "2026-08-26T00:00:00Z"
            ),
            componentKey: "computer-use",
            serverKey: nil,
            adapterSessionID: "adapter-1",
            ownerUserID: "user-1",
            deviceID: "device-1",
            workspaceRoot: root,
            permissionSnapshot: ["process.spawn"],
            runtimeRootURL: root.appendingPathComponent("runtime", isDirectory: true)
        )

        #expect(launch.executableURL == launcher.standardizedFileURL)
        #expect(launch.arguments == ["mcp"])
        #expect(launch.environment["CHATOS_WORKSPACE"] == root.path)
#if os(macOS)
        #expect(launch.environment["OPEN_COMPUTER_USE_MANAGED_APP_ROOT"]?.hasSuffix("/Open Computer Use/runtime") == true)
#endif
    }

    @Test("device-only plugin launch does not receive a workspace environment path")
    func deviceOnlyPluginLaunchOmitsWorkspaceEnvironment() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let installation = root.appendingPathComponent("plugin", isDirectory: true)
        let launcher = installation
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent("fixture")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(
            at: launcher.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("#!/bin/sh\n".utf8).write(to: launcher)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: launcher.path)
        try Data("""
        {"schemaVersion":3,"name":"fixture","version":"1.0.0","mcpServers":{"fixture":{"type":"stdio","bin":"fixture"}},"permissions":[{"permission":"process.spawn","required":true,"components":["fixture"]}]}
        """.utf8).write(to: installation.appendingPathComponent("chatos.plugin.json"))

        let launch = try NativePluginManifestLoader.prepare(
            record: .init(
                pluginID: "plugin-1",
                releaseID: "release-1",
                version: "1.0.0",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: installation.path,
                installedAt: "2026-08-29T00:00:00Z"
            ),
            componentKey: "fixture",
            serverKey: nil,
            adapterSessionID: "adapter-device-only",
            ownerUserID: "user-1",
            deviceID: "device-1",
            workspaceRoot: nil,
            permissionSnapshot: ["process.spawn"],
            runtimeRootURL: root.appendingPathComponent("runtime", isDirectory: true)
        )

        #expect(launch.environment["CHATOS_WORKSPACE"] == nil)
    }

    @Test("stdio client initializes, lists tools and calls a tool")
    func stdioRoundTrip() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let script = root.appendingPathComponent("fixture.zsh")
        try """
        while IFS= read -r line; do
          if [[ "$line" == *'tools/list'* ]]; then
            echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"echo","description":"Echo","inputSchema":{"type":"object"}}]}}'
          elif [[ "$line" == *'tools/call'* ]]; then
            echo '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"ok"}]}}'
          elif [[ "$line" == *'initialize'* ]]; then
            echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"instructions":"Fixture instructions"}}'
          fi
        done
        """.write(to: script, atomically: true, encoding: .utf8)
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {"schemaVersion":3,"name":"fixture","version":"1.0.0","mcpServers":{"fixture":{"type":"stdio","bin":"fixture","args":[]}}}
            """.utf8)
        )
        let launch = NativePreparedPluginLaunch(
            manifest: manifest,
            componentKey: "fixture",
            server: manifest.mcpServers["fixture"]!,
            executableURL: URL(fileURLWithPath: "/bin/zsh"),
            arguments: [script.path],
            environment: [:],
            installationURL: root,
            visualSessionURL: root.appendingPathComponent("visual"),
            artifactURL: root.appendingPathComponent("artifacts"),
            displayName: "Fixture"
        )
        let client = NativePluginStdioClient(launch: launch)
        try await client.start()
        let prepared = try await client.initialize()
        #expect(prepared.instructions == "Fixture instructions")
        #expect(prepared.tools.first?.jsonObject?["name"]?.jsonString == "echo")
        let result = try await client.callTool(
            name: "echo",
            arguments: .object(["text": .string("hello")]),
            timeout: .seconds(2)
        )
        #expect(result.jsonObject?["content"]?.jsonArray?.first?.jsonObject?["text"]?.jsonString == "ok")
        await client.terminate()
    }

    @Test("stdio client preserves the byte order of a large chunked response")
    func stdioLargeChunkedResponse() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let script = root.appendingPathComponent("fixture.zsh")
        try """
        while IFS= read -r line; do
          if [[ "$line" == *'tools/list'* ]]; then
            echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"large","description":"Large response","inputSchema":{"type":"object"}}]}}'
          elif [[ "$line" == *'tools/call'* ]]; then
            /usr/bin/awk 'BEGIN {
              printf "{\\\"jsonrpc\\\":\\\"2.0\\\",\\\"id\\\":3,\\\"result\\\":{\\\"content\\\":[{\\\"type\\\":\\\"text\\\",\\\"text\\\":\\\""
              for (i = 0; i < 524288; i++) printf "x"
              printf "\\\"}]}}\\n"
            }'
          elif [[ "$line" == *'initialize'* ]]; then
            echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}}}}'
          fi
        done
        """.write(to: script, atomically: true, encoding: .utf8)
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {"schemaVersion":3,"name":"fixture","version":"1.0.0","mcpServers":{"fixture":{"type":"stdio","bin":"fixture","args":[]}}}
            """.utf8)
        )
        let launch = NativePreparedPluginLaunch(
            manifest: manifest,
            componentKey: "fixture",
            server: manifest.mcpServers["fixture"]!,
            executableURL: URL(fileURLWithPath: "/bin/zsh"),
            arguments: [script.path],
            environment: [:],
            installationURL: root,
            visualSessionURL: root.appendingPathComponent("visual"),
            artifactURL: root.appendingPathComponent("artifacts"),
            displayName: "Fixture"
        )
        let client = NativePluginStdioClient(launch: launch)
        try await client.start()
        _ = try await client.initialize()
        let result = try await client.callTool(
            name: "large",
            arguments: .object([:]),
            timeout: .seconds(5)
        )
        let text = try #require(
            result.jsonObject?["content"]?.jsonArray?.first?.jsonObject?["text"]?.jsonString
        )
        #expect(text.count == 524_288)
        #expect(text.allSatisfy { $0 == "x" })
        await client.terminate()
    }

    @Test(
        "installed Browser CDP completes the real Swift stdio open, navigate, screenshot and snapshot path",
        .enabled(if: ProcessInfo.processInfo.environment["CHATOS_BROWSER_CDP_INTEGRATION_ROOT"] != nil)
    )
    func installedBrowserCDPSwiftStdioIntegration() async throws {
        let installationPath = try #require(
            ProcessInfo.processInfo.environment["CHATOS_BROWSER_CDP_INTEGRATION_ROOT"]
        )
        let installation = URL(fileURLWithPath: installationPath, isDirectory: true)
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data(contentsOf: installation.appendingPathComponent("chatos.plugin.json"))
        )
        let runtimeRoot = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: runtimeRoot) }
        let launch = try NativePluginManifestLoader.prepare(
            record: .init(
                pluginID: "browser-cdp-integration",
                releaseID: "browser-cdp-integration-release",
                version: manifest.version,
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: installation.path,
                installedAt: "2026-08-29T00:00:00Z"
            ),
            componentKey: "browser-cdp",
            serverKey: nil,
            adapterSessionID: UUID().uuidString.lowercased(),
            ownerUserID: "user-1",
            deviceID: "device-1",
            workspaceRoot: nil,
            permissionSnapshot: Set(manifest.permissions.map(\.permission)),
            runtimeRootURL: runtimeRoot
        )
        let client = NativePluginStdioClient(launch: launch)
        try await client.start()
        _ = try await client.initialize()

        do {
            let opened = try await client.callTool(
                name: "browser_session_open",
                arguments: .object([
                    "mode": .string("managed"),
                    "persistent_profile": .bool(false),
                    "headless": .bool(true),
                    "session_name": .string("Swift stdio integration"),
                ]),
                timeout: .seconds(60)
            )
            #expect(opened.jsonObject?["isError"]?.jsonBool != true)

            let navigated = try await client.callTool(
                name: "browser_navigate",
                arguments: .object([
                    "timeout_ms": .number(30_000),
                    "url": .string("https://github.com/search?q=%22DeepSeek+Harness%22&type=repositories"),
                ]),
                timeout: .seconds(45)
            )
            #expect(navigated.jsonObject?["isError"]?.jsonBool != true)

            let screenshot = try await client.callTool(
                name: "browser_screenshot",
                arguments: .object(["full_page": .bool(false)]),
                timeout: .seconds(30)
            )
            #expect(screenshot.jsonObject?["isError"]?.jsonBool != true)

            let snapshot = try await client.callTool(
                name: "browser_snapshot",
                arguments: .object([:]),
                timeout: .seconds(30)
            )
            #expect(snapshot.jsonObject?["isError"]?.jsonBool != true)
            #expect(snapshot.jsonObject?["structuredContent"]?.jsonArray?.isEmpty == false)

            let status = try await client.callTool(
                name: "browser_session_status",
                arguments: .object([:]),
                timeout: .seconds(10)
            )
            #expect(status.jsonObject?["structuredContent"]?.jsonObject?["state"]?.jsonString == "open")
            _ = try await client.callTool(
                name: "browser_session_close",
                arguments: .object([:]),
                timeout: .seconds(10)
            )
        } catch {
            _ = try? await client.callTool(
                name: "browser_session_close",
                arguments: .object([:]),
                timeout: .seconds(5)
            )
            await client.terminate()
            throw error
        }
        await client.terminate()
    }

    @Test("stdio hard timeout terminates a wedged plugin session")
    func stdioTimeoutTerminatesWedgedSession() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let script = root.appendingPathComponent("fixture.zsh")
        try """
        while IFS= read -r line; do
          if [[ "$line" == *'tools/list'* ]]; then
            echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"hang","description":"Hang","inputSchema":{"type":"object"}}]}}'
          elif [[ "$line" == *'tools/call'* ]]; then
            while true; do :; done
          elif [[ "$line" == *'initialize'* ]]; then
            echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}}}}'
          fi
        done
        """.write(to: script, atomically: true, encoding: .utf8)
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {"schemaVersion":3,"name":"fixture","version":"1.0.0","mcpServers":{"fixture":{"type":"stdio","bin":"fixture","args":[]}}}
            """.utf8)
        )
        let launch = NativePreparedPluginLaunch(
            manifest: manifest,
            componentKey: "fixture",
            server: manifest.mcpServers["fixture"]!,
            executableURL: URL(fileURLWithPath: "/bin/zsh"),
            arguments: [script.path],
            environment: [:],
            installationURL: root,
            visualSessionURL: root.appendingPathComponent("visual"),
            artifactURL: root.appendingPathComponent("artifacts"),
            displayName: "Fixture"
        )
        let client = NativePluginStdioClient(launch: launch)
        try await client.start()
        _ = try await client.initialize()

        do {
            _ = try await client.callTool(
                name: "hang",
                arguments: .object([:]),
                timeout: .milliseconds(100)
            )
            Issue.record("Expected the wedged call to time out")
        } catch let error as NativePluginRuntimeError {
            #expect(error.errorDescription == NativePluginRuntimeError.timeout.errorDescription)
        }

        do {
            _ = try await client.callTool(
                name: "hang",
                arguments: .object([:]),
                timeout: .seconds(1)
            )
            Issue.record("Expected the timed-out process session to stay unavailable")
        } catch let error as NativePluginRuntimeError {
            #expect(error.errorDescription == NativePluginRuntimeError.processUnavailable.errorDescription)
        }
    }

    @Test("read-only browser tools skip redundant visual refresh and a timed-out action refresh keeps the MCP process alive")
    func browserVisualRefreshTimeoutKeepsPluginSessionAlive() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let visual = root.appendingPathComponent("visual", isDirectory: true)
        let artifacts = root.appendingPathComponent("artifacts", isDirectory: true)
        try FileManager.default.createDirectory(at: visual, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: artifacts, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let script = root.appendingPathComponent("fixture.zsh")
        let screenshotCalls = root.appendingPathComponent("screenshot-calls.log")
        try """
        while IFS= read -r line; do
          if [[ "$line" == *'tools/list'* ]]; then
            echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"browser_snapshot"},{"name":"browser_screenshot"},{"name":"browser_click"},{"name":"browser_session_status"}]}}'
          elif [[ "$line" == *'"name":"browser_snapshot"'* ]]; then
            echo '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"[]"}],"structuredContent":[]}}'
          elif [[ "$line" == *'"name":"browser_screenshot"'* ]]; then
            echo "$line" >> '\(screenshotCalls.path)'
            : # Deliberately omit a response so the best-effort refresh times out.
          elif [[ "$line" == *'"name":"browser_click"'* ]]; then
            echo '{"jsonrpc":"2.0","id":4,"result":{"content":[{"type":"text","text":"clicked"}],"structuredContent":{"clicked":true}}}'
          elif [[ "$line" == *'"name":"browser_session_status"'* ]]; then
            echo '{"jsonrpc":"2.0","id":6,"result":{"content":[{"type":"text","text":"open"}],"structuredContent":{"state":"open"}}}'
          elif [[ "$line" == *'initialize'* ]]; then
            echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}}}}'
          fi
        done
        """.write(to: script, atomically: true, encoding: .utf8)
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {"schemaVersion":3,"name":"fixture","version":"1.0.0","mcpServers":{"browser-cdp":{"type":"stdio","bin":"fixture","args":[]}}}
            """.utf8)
        )
        let launch = NativePreparedPluginLaunch(
            manifest: manifest,
            componentKey: "browser-cdp",
            server: manifest.mcpServers["browser-cdp"]!,
            executableURL: URL(fileURLWithPath: "/bin/zsh"),
            arguments: [script.path],
            environment: [:],
            installationURL: root,
            visualSessionURL: visual,
            artifactURL: artifacts,
            displayName: "Browser fixture"
        )
        let client = NativePluginStdioClient(launch: launch)
        try await client.start()
        let initialized = try await client.initialize()
        let store = NativePluginRuntimeStore(browserVisualRefreshTimeout: .milliseconds(100))
        let identity = NativePluginRuntimeStore.Identity(
            runID: "run-1",
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "1.0.0",
            artifactSHA256: String(repeating: "a", count: 64),
            componentKey: "browser-cdp",
            adapterSessionID: "adapter-1",
            projectID: "project-1"
        )
        await store.insert(
            identity: identity,
            client: client,
            tools: initialized.tools,
            permissionSnapshot: ["browser.file.transfer"],
            displayName: "Browser fixture",
            visualSessionURL: visual,
            artifactURL: artifacts,
            projectRootURL: root,
            workspaceID: "workspace-1"
        )
        _ = try await store.validate(
            adapterSessionID: identity.adapterSessionID,
            pluginID: identity.pluginID,
            releaseID: identity.releaseID,
            artifactSHA256: identity.artifactSHA256,
            componentKey: identity.componentKey,
            workspaceID: "workspace-1",
            projectID: "project-1"
        )
        do {
            _ = try await store.validate(
                adapterSessionID: identity.adapterSessionID,
                pluginID: identity.pluginID,
                releaseID: identity.releaseID,
                artifactSHA256: identity.artifactSHA256,
                componentKey: identity.componentKey,
                workspaceID: nil,
                projectID: "project-1"
            )
            Issue.record("project-scoped plugin session accepted a device-only execute scope")
        } catch is NativePluginRuntimeError {
            // Expected: prepare and execute/cancel must retain the same relay scope.
        }
        do {
            try await store.validateScopeIfPresent(
                adapterSessionID: identity.adapterSessionID,
                workspaceID: nil,
                projectID: "project-1"
            )
            Issue.record("project-scoped plugin session accepted a device-only cancel scope")
        } catch is NativePluginRuntimeError {
            // Expected.
        }
        do {
            _ = try await store.validate(
                adapterSessionID: identity.adapterSessionID,
                pluginID: identity.pluginID,
                releaseID: identity.releaseID,
                artifactSHA256: identity.artifactSHA256,
                componentKey: identity.componentKey,
                workspaceID: "workspace-1",
                projectID: "project-2"
            )
            Issue.record("project-scoped plugin session accepted a different project id")
        } catch is NativePluginRuntimeError {
            // Expected: AI calls cannot switch a prepared session to another project.
        }

        _ = try await store.call(
            adapterSessionID: identity.adapterSessionID,
            invocationID: "snapshot-1",
            toolName: "browser_snapshot",
            arguments: .object([:]),
            timeout: .seconds(1)
        )
        #expect(!FileManager.default.fileExists(atPath: screenshotCalls.path))
        let clicked = try await store.call(
            adapterSessionID: identity.adapterSessionID,
            invocationID: "click-1",
            toolName: "browser_click",
            arguments: .object([:]),
            timeout: .seconds(1)
        )

        #expect(clicked.jsonObject?["structuredContent"]?.jsonObject?["clicked"]?.jsonBool == true)
        let screenshotCall = try String(contentsOf: screenshotCalls, encoding: .utf8)
        #expect(screenshotCall.split(separator: "\n").count == 1)
        #expect(screenshotCall.contains("\"full_page\":false"))
        #expect(!screenshotCall.contains("browser_session_id"))
        let status = try await store.call(
            adapterSessionID: identity.adapterSessionID,
            invocationID: "status-1",
            toolName: "browser_session_status",
            arguments: .object([:]),
            timeout: .seconds(1)
        )
        #expect(status.jsonObject?["structuredContent"]?.jsonObject?["state"]?.jsonString == "open")
        await store.terminateAll()
    }

    @Test("stdio task cancellation terminates a wedged plugin session")
    func stdioCancellationTerminatesWedgedSession() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let script = root.appendingPathComponent("fixture.zsh")
        try """
        while IFS= read -r line; do
          if [[ "$line" == *'tools/list'* ]]; then
            echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"hang","description":"Hang","inputSchema":{"type":"object"}}]}}'
          elif [[ "$line" == *'tools/call'* ]]; then
            while true; do :; done
          elif [[ "$line" == *'initialize'* ]]; then
            echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}}}}'
          fi
        done
        """.write(to: script, atomically: true, encoding: .utf8)
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {"schemaVersion":3,"name":"fixture","version":"1.0.0","mcpServers":{"fixture":{"type":"stdio","bin":"fixture","args":[]}}}
            """.utf8)
        )
        let launch = NativePreparedPluginLaunch(
            manifest: manifest,
            componentKey: "fixture",
            server: manifest.mcpServers["fixture"]!,
            executableURL: URL(fileURLWithPath: "/bin/zsh"),
            arguments: [script.path],
            environment: [:],
            installationURL: root,
            visualSessionURL: root.appendingPathComponent("visual"),
            artifactURL: root.appendingPathComponent("artifacts"),
            displayName: "Fixture"
        )
        let client = NativePluginStdioClient(launch: launch)
        try await client.start()
        _ = try await client.initialize()

        let call = Task {
            try await client.callTool(
                name: "hang",
                arguments: .object([:]),
                timeout: .seconds(10)
            )
        }
        try await Task.sleep(for: .milliseconds(100))
        call.cancel()

        do {
            _ = try await call.value
            Issue.record("Expected the wedged call to be cancelled")
        } catch let error as NativePluginRuntimeError {
            #expect(error.errorDescription == NativePluginRuntimeError.cancelled.errorDescription)
        }

        do {
            _ = try await client.callTool(
                name: "hang",
                arguments: .object([:]),
                timeout: .seconds(1)
            )
            Issue.record("Expected the cancelled process session to stay unavailable")
        } catch let error as NativePluginRuntimeError {
            #expect(error.errorDescription == NativePluginRuntimeError.processUnavailable.errorDescription)
        }
    }

    @Test("plugin host deadline keeps the two hour task execution contract")
    func pluginToolTimeoutPolicyUsesTaskExecutionCeiling() {
        #expect(NativeLocalConnectorService.defaultPluginToolTimeoutMilliseconds(
            componentKey: "computer-use",
            toolName: "click"
        ) == 7_200_000)
        #expect(NativeLocalConnectorService.defaultPluginToolTimeoutMilliseconds(
            componentKey: "computer-use",
            toolName: "get_app_state"
        ) == 7_200_000)
        #expect(NativeLocalConnectorService.defaultPluginToolTimeoutMilliseconds(
            componentKey: "browser-cdp",
            toolName: "browser_navigate"
        ) == 7_200_000)
        #expect(NativeLocalConnectorService.pluginToolHostTimeoutMilliseconds(
            declaredTimeoutMilliseconds: 20_000
        ) == 30_000)
        #expect(NativeLocalConnectorService.pluginToolHostTimeoutMilliseconds(
            declaredTimeoutMilliseconds: 5_000
        ) == 7_500)
        #expect(NativeLocalConnectorService.pluginToolHostTimeoutMilliseconds(
            declaredTimeoutMilliseconds: 7_200_000
        ) == 7_200_000)
    }

    @Test("browser screenshot artifacts become local visual-session frames")
    func browserArtifactBridge() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let artifacts = root.appendingPathComponent("artifacts", isDirectory: true)
        let visual = root.appendingPathComponent("visual", isDirectory: true)
        try FileManager.default.createDirectory(at: artifacts, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: visual, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let frame = Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")!
        try frame.write(to: artifacts.appendingPathComponent("capture.png"))
        let result = NativeJSONValue.object([
            "content": .array([
                .object([
                    "type": .string("text"),
                    "text": .string("{\"browser_session_id\":\"browser-1\",\"relative_path\":\"capture.png\"}"),
                ]),
            ]),
        ])
        #expect(NativeBrowserVisualBridge.browserSessionID(arguments: .object([:]), result: result) == "browser-1")
        #expect(NativeBrowserVisualBridge.captureFrame(from: result, artifactRootURL: artifacts) == frame)
        try NativeBrowserVisualBridge.publish(
            frame: frame,
            adapterSessionID: "adapter-1",
            visualSessionURL: visual,
            sequence: 3,
            target: "example.com"
        )
        #expect(FileManager.default.fileExists(atPath: visual.appendingPathComponent("frame.png").path))
        let metadata = try JSONDecoder().decode(
            NativeJSONValue.self,
            from: Data(contentsOf: visual.appendingPathComponent("session.json"))
        )
        #expect(metadata.jsonObject?["frame_sequence"]?.jsonNumber == 3)
        #expect(metadata.jsonObject?["target_app"]?.jsonString == "example.com")
    }

    @Test("MCP Office Artifact candidates are validated and persisted in the project")
    func officeArtifactRegistration() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let workspace = root.appendingPathComponent("workspace", isDirectory: true)
        let artifacts = root.appendingPathComponent("artifacts", isDirectory: true)
        try FileManager.default.createDirectory(at: workspace, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: artifacts, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let bytes = Data("docx-fixture".utf8)
        try bytes.write(to: artifacts.appendingPathComponent("report.docx"))
        let sha256 = NativePluginHash.sha256(bytes)
        let identity = NativePluginRuntimeStore.Identity(
            runID: "run-1",
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "0.1.1",
            artifactSHA256: String(repeating: "a", count: 64),
            componentKey: "document-mcp",
            adapterSessionID: "adapter-1"
        )
        let registered = try NativePluginArtifactRegistrar.register(
            result: .object([
                "content": .array([]),
                "_meta": .object([
                    "chatos/artifacts": .array([
                        .object([
                            "producer_artifact_id": .string("document-local-1"),
                            "relative_path": .string("report.docx"),
                            "display_name": .string("report.docx"),
                            "media_type": .string("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
                            "size_bytes": .number(Double(bytes.count)),
                            "sha256": .string(sha256),
                        ]),
                    ]),
                ]),
            ]),
            identity: identity,
            ownerUserID: "user-1",
            deviceID: "device-1",
            workspaceID: "workspace-1",
            workspaceRootURL: workspace,
            artifactRootURL: artifacts,
            permissionSnapshot: ["artifact.create"],
            toolName: "office_create"
        )
        let authoritative = try #require(
            registered.jsonObject?["_meta"]?.jsonObject?["chatos/artifacts"]?.jsonArray?.first?.jsonObject
        )
        #expect(authoritative["producer_artifact_id"]?.jsonString == "document-local-1")
        let descriptor = try #require(authoritative["artifact"]?.jsonObject)
        #expect(descriptor["media_type"]?.jsonString == "application/vnd.openxmlformats-officedocument.wordprocessingml.document")
        let relativePath = try #require(descriptor["workspace_relative_path"]?.jsonString)
        #expect(relativePath.hasPrefix("chatos-plugin-artifacts/adapter-1/pa_"))
        #expect((try Data(contentsOf: workspace.appendingPathComponent(relativePath))) == bytes)
    }

    @Test("computer use image blocks become local visual-session frames")
    func computerUseImageBridge() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let frame = Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")!
        let result = NativeJSONValue.object([
            "content": .array([
                .object([
                    "type": .string("text"),
                    "text": .string("Window state"),
                ]),
                .object([
                    "type": .string("image"),
                    "mimeType": .string("image/png"),
                    "data": .string(frame.base64EncodedString()),
                ]),
            ]),
        ])

        #expect(NativeComputerUseVisualBridge.captureFrame(from: result)?.data == frame)
        #expect(NativeComputerUseVisualBridge.targetApplication(
            arguments: .object(["app": .string("飞书")])
        ) == "飞书")
        #expect(NativeComputerUseVisualBridge.targetApplication(
            arguments: .object(["app_name": .string("飞书")])
        ) == "飞书")
        #expect(NativeComputerUseVisualBridge.targetApplication(
            arguments: .object(["bundle_id": .string("com.electron.lark")])
        ) == "com.electron.lark")
        try NativeComputerUseVisualBridge.publish(
            frame: .init(data: frame, mimeType: "image/png", fileName: "frame.png"),
            adapterSessionID: "adapter-1",
            visualSessionURL: root,
            sequence: 4,
            targetApplication: "飞书"
        )

        #expect(FileManager.default.fileExists(atPath: root.appendingPathComponent("frame.png").path))
        let metadata = try JSONDecoder().decode(
            NativeJSONValue.self,
            from: Data(contentsOf: root.appendingPathComponent("session.json"))
        )
        #expect(metadata.jsonObject?["session_id"]?.jsonString == "computer-adapter-1")
        #expect(metadata.jsonObject?["frame_sequence"]?.jsonNumber == 4)
        #expect(metadata.jsonObject?["target_app"]?.jsonString == "飞书")
    }

    @Test("computer use JPEG image blocks retain their native format")
    func computerUseJPEGImageBridge() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let frame = Data([0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0xFF, 0xD9])
        let result = NativeJSONValue.object([
            "content": .array([
                .object([
                    "type": .string("image"),
                    "mimeType": .string("image/jpeg"),
                    "data": .string(frame.base64EncodedString()),
                ]),
            ]),
        ])

        let captured = try #require(NativeComputerUseVisualBridge.captureFrame(from: result))
        #expect(captured.data == frame)
        #expect(captured.mimeType == "image/jpeg")
        #expect(captured.fileName == "frame.jpg")
        try NativeComputerUseVisualBridge.publish(
            frame: captured,
            adapterSessionID: "adapter-jpeg",
            visualSessionURL: root,
            sequence: 5,
            targetApplication: "飞书"
        )

        #expect(FileManager.default.fileExists(atPath: root.appendingPathComponent("frame.jpg").path))
        let metadata = try JSONDecoder().decode(
            NativeJSONValue.self,
            from: Data(contentsOf: root.appendingPathComponent("session.json"))
        )
        #expect(metadata.jsonObject?["mime_type"]?.jsonString == "image/jpeg")
        #expect(metadata.jsonObject?["frame_file"]?.jsonString == "frame.jpg")
    }

    @Test("visual frame remains visible for the lifetime of its active plugin session")
    func visualFrameLifetimeFollowsSessionInsteadOfFifteenSecondCache() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let frame = Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")!
        try frame.write(to: root.appendingPathComponent("frame.png"))
        let adapterSessionID = "adapter-active"
        try Data("""
        {"protocol_version":1,"adapter_session_id":"\(adapterSessionID)","plugin_id":"plugin-1","component_key":"computer-use"}
        """.utf8).write(to: root.appendingPathComponent("host.json"))
        try Data("""
        {"protocol_version":1,"session_id":"computer-\(adapterSessionID)","status":"running","title":"电脑操作","target_app":"飞书","mime_type":"image/png","frame_file":"frame.png","frame_sequence":9,"captured_at":"2026-08-26T03:00:00Z"}
        """.utf8).write(to: root.appendingPathComponent("session.json"))
        let identity = NativePluginRuntimeStore.Identity(
            runID: "run-1",
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "1.0.0",
            artifactSHA256: String(repeating: "a", count: 64),
            componentKey: "computer-use",
            adapterSessionID: adapterSessionID
        )
        let sessions = NativePluginVisualSessionReader.read(
            descriptors: [
                .init(
                    identity: identity,
                    displayName: "Open Computer Use",
                    visualSessionURL: root,
                    owner: .init(conversationID: "conversation-1"),
                    ownerBoundAt: Date(timeIntervalSince1970: 1_777_000_000)
                ),
            ],
            now: ISO8601DateFormatter().date(from: "2026-08-26T05:00:00Z")!
        )

        #expect(sessions.count == 1)
        #expect(sessions.first?.frameData == frame)
        #expect(sessions.first?.frameSequence == 9)
    }

    @Test("multiple visual sessions remain discoverable while only the selected frame bytes are loaded")
    func multipleVisualSessionsLoadFrameDataOnDemand() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let frame = Data(base64Encoded: "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")!

        func descriptor(
            adapterSessionID: String,
            componentKey: String,
            taskTitle: String,
            boundAt: Date
        ) throws -> NativePluginRuntimeStore.VisualDescriptor {
            let directory = root.appendingPathComponent(adapterSessionID, isDirectory: true)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            try frame.write(to: directory.appendingPathComponent("frame.png"))
            try Data("""
            {"protocol_version":1,"adapter_session_id":"\(adapterSessionID)","plugin_id":"plugin-1","component_key":"\(componentKey)"}
            """.utf8).write(to: directory.appendingPathComponent("host.json"))
            try Data("""
            {"protocol_version":1,"session_id":"visual-\(adapterSessionID)","status":"running","title":"实时操作","mime_type":"image/png","frame_file":"frame.png","frame_sequence":3,"captured_at":"2026-08-28T03:00:00Z"}
            """.utf8).write(to: directory.appendingPathComponent("session.json"))
            return .init(
                identity: .init(
                    runID: "run-\(adapterSessionID)",
                    pluginID: "plugin-1",
                    releaseID: "release-1",
                    version: "1.0.0",
                    artifactSHA256: String(repeating: "a", count: 64),
                    componentKey: componentKey,
                    adapterSessionID: adapterSessionID
                ),
                displayName: componentKey,
                visualSessionURL: directory,
                owner: .init(
                    conversationID: "conversation-1",
                    taskRunID: "run-\(adapterSessionID)",
                    taskTitle: taskTitle
                ),
                ownerBoundAt: boundAt
            )
        }

        let sessions = NativePluginVisualSessionReader.read(
            descriptors: [
                try descriptor(
                    adapterSessionID: "adapter-computer",
                    componentKey: "computer-use",
                    taskTitle: "整理桌面文件",
                    boundAt: Date(timeIntervalSince1970: 10)
                ),
                try descriptor(
                    adapterSessionID: "adapter-browser",
                    componentKey: "browser-cdp",
                    taskTitle: "检查网站",
                    boundAt: Date(timeIntervalSince1970: 20)
                ),
            ],
            now: ISO8601DateFormatter().date(from: "2026-08-28T03:00:01Z")!,
            loadFrameDataForAdapterSessionIDs: ["adapter-browser"]
        )

        #expect(sessions.count == 2)
        #expect(sessions.first(where: { $0.adapterSessionID == "adapter-browser" })?.frameData == frame)
        #expect(sessions.first(where: { $0.adapterSessionID == "adapter-computer" })?.frameData == nil)
        #expect(sessions.first(where: { $0.adapterSessionID == "adapter-computer" })?.owner.taskTitle == "整理桌面文件")
    }

    @Test("computer use adapters hold an exclusive desktop lease until their session closes")
    func computerUseAdaptersAreSerializedAcrossTaskRuns() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let first = try await makeComputerUseLeaseClient(root: root, label: "first")
        let second = try await makeComputerUseLeaseClient(root: root, label: "second")
        let store = NativePluginRuntimeStore()
        for (adapterSessionID, fixture) in [("adapter-first", first), ("adapter-second", second)] {
            await store.insert(
                identity: .init(
                    runID: "run-\(adapterSessionID)",
                    pluginID: "plugin-1",
                    releaseID: "release-1",
                    version: "1.0.0",
                    artifactSHA256: String(repeating: "a", count: 64),
                    componentKey: "desktop-control",
                    adapterSessionID: adapterSessionID,
                    requiresExclusiveExecution: true
                ),
                client: fixture.0,
                tools: fixture.1,
                permissionSnapshot: [],
                displayName: adapterSessionID,
                visualSessionURL: fixture.2,
                artifactURL: fixture.3,
                projectRootURL: root,
                workspaceID: "workspace-1"
            )
        }

        _ = try await store.call(
            adapterSessionID: "adapter-first",
            invocationID: "first-call",
            toolName: "observe",
            arguments: .object([:]),
            timeout: .seconds(2)
        )
        let secondCall = Task {
            try await store.call(
                adapterSessionID: "adapter-second",
                invocationID: "second-call",
                toolName: "observe",
                arguments: .object([:]),
                timeout: .seconds(2)
            )
        }
        try await Task.sleep(for: .milliseconds(100))
        #expect(!FileManager.default.fileExists(atPath: second.4.path))

        #expect(await store.cancel(adapterSessionID: "adapter-first", invocationID: nil) == "cancelled")
        let secondResult = try await secondCall.value
        #expect(secondResult.jsonObject?["content"]?.jsonArray?.first?.jsonObject?["text"]?.jsonString == "ok-second")
        #expect(FileManager.default.fileExists(atPath: second.4.path))
        #expect(await store.cancel(adapterSessionID: "adapter-second", invocationID: nil) == "cancelled")
    }

    @Test("a hard computer use call failure releases the desktop lease for the next adapter")
    func computerUseFailureReleasesLeaseForNextTaskRun() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let first = try await makeComputerUseLeaseClient(
            root: root,
            label: "first-failing",
            behavior: .hang
        )
        let second = try await makeComputerUseLeaseClient(root: root, label: "second-after-failure")
        let store = NativePluginRuntimeStore()
        for (adapterSessionID, fixture) in [("adapter-first", first), ("adapter-second", second)] {
            await store.insert(
                identity: .init(
                    runID: "run-\(adapterSessionID)",
                    pluginID: "plugin-1",
                    releaseID: "release-1",
                    version: "1.0.0",
                    artifactSHA256: String(repeating: "a", count: 64),
                    componentKey: "desktop-control",
                    adapterSessionID: adapterSessionID,
                    requiresExclusiveExecution: true
                ),
                client: fixture.0,
                tools: fixture.1,
                permissionSnapshot: [],
                displayName: adapterSessionID,
                visualSessionURL: fixture.2,
                artifactURL: fixture.3,
                projectRootURL: root,
                workspaceID: "workspace-1"
            )
        }

        let firstCall = Task {
            try await store.call(
                adapterSessionID: "adapter-first",
                invocationID: "first-call",
                toolName: "observe",
                arguments: .object([:]),
                timeout: .milliseconds(150)
            )
        }
        try await Task.sleep(for: .milliseconds(35))
        let secondCall = Task {
            try await store.call(
                adapterSessionID: "adapter-second",
                invocationID: "second-call",
                toolName: "observe",
                arguments: .object([:]),
                timeout: .seconds(2)
            )
        }
        try await Task.sleep(for: .milliseconds(60))
        #expect(!FileManager.default.fileExists(atPath: second.4.path))

        do {
            _ = try await firstCall.value
            Issue.record("expected the first computer use call to time out")
        } catch let error as NativePluginRuntimeError {
            #expect(error.errorDescription == NativePluginRuntimeError.timeout.errorDescription)
        }

        let secondResult = try await secondCall.value
        #expect(
            secondResult.jsonObject?["content"]?.jsonArray?.first?
                .jsonObject?["text"]?.jsonString == "ok-second-after-failure"
        )
        #expect(FileManager.default.fileExists(atPath: second.4.path))
        #expect(await store.cancel(adapterSessionID: "adapter-first", invocationID: nil) == "cancelled")
        #expect(await store.cancel(adapterSessionID: "adapter-second", invocationID: nil) == "cancelled")
    }

    @Test("plugins without an actual frame do not enter picture in picture")
    func visualSessionRequiresDisplayableFrame() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let adapterSessionID = "adapter-document"
        try Data("""
        {"protocol_version":1,"adapter_session_id":"\(adapterSessionID)","plugin_id":"plugin-1","component_key":"document-mcp"}
        """.utf8).write(to: root.appendingPathComponent("host.json"))
        let identity = NativePluginRuntimeStore.Identity(
            runID: "run-1",
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "0.1.1",
            artifactSHA256: String(repeating: "a", count: 64),
            componentKey: "document-mcp",
            adapterSessionID: adapterSessionID
        )
        let descriptor = NativePluginRuntimeStore.VisualDescriptor(
            identity: identity,
            displayName: "Document Tools",
            visualSessionURL: root,
            owner: .init(conversationID: "conversation-1"),
            ownerBoundAt: Date()
        )

        #expect(NativePluginVisualSessionReader.read(descriptors: [descriptor]).isEmpty)

        try Data("""
        {"protocol_version":1,"session_id":"document-1","status":"running","title":"文档处理","mime_type":"image/png","frame_file":"frame.png","frame_sequence":1,"captured_at":"2026-08-27T01:00:00Z"}
        """.utf8).write(to: root.appendingPathComponent("session.json"))
        let now = ISO8601DateFormatter().date(from: "2026-08-27T01:00:01Z")!
        #expect(NativePluginVisualSessionReader.read(descriptors: [descriptor], now: now).isEmpty)
    }

    @Test("plugin permissions use the installed app's real diagnostic state")
    func pluginPermissionDiagnostics() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let launcher = bin.appendingPathComponent("open-computer-use")
        try Data("""
        #!/bin/sh
        # check-permissions
        printf '%s\\n' '{"permissions":[{"kind":"accessibility","title":"辅助功能","granted":true,"purpose":"发送输入","systemSettingsTitle":"隐私与安全性 > 辅助功能"},{"kind":"screenRecording","title":"屏幕与系统音频录制","granted":false,"purpose":"读取画面","systemSettingsTitle":"隐私与安全性 > 屏幕与系统音频录制"}]}'
        """.utf8).write(to: launcher)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: launcher.path
        )
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {
              "schemaVersion": 3,
              "name": "open-computer-use",
              "version": "0.8.1",
              "mcpServers": {"computer-use":{"type":"stdio","bin":"open-computer-use"}},
              "permissions": [
                {"permission":"process.spawn","required":true,"reason":"启动 MCP","components":["computer-use"]},
                {"permission":"computer.control","required":true,"reason":"控制桌面","components":["computer-use"]}
              ]
            }
            """.utf8)
        )
        let record = NativeInstalledPluginRecord(
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "0.8.1",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: root.path,
            installedAt: "2026-08-27T00:00:00Z"
        )

        let permissions = NativePluginPermissionInspector.permissions(
            record: record,
            manifest: manifest
        )

        #expect(permissions.count == 4)
        #expect(permissions.map(\.permissionID) == [
            "process.spawn",
            "computer.control",
            "computer.accessibility",
            "computer.screen-recording",
        ])
        #expect(permissions[0].statusLabel == "已可用")
        #expect(permissions[1].statusLabel == "已可用")
        #expect(permissions[2].status == "ready")
        #expect(permissions[2].statusLabel == "已允许")
        #expect(permissions[3].status == "action_required")
        #expect(permissions[3].canRequest)
        #expect(permissions[3].requestLabel == "去开启")
    }

    @Test("plugin processes restore tool paths missing from GUI app launches")
    func pluginProcessEnvironmentRestoresToolPaths() {
        let environment = NativePluginProcessEnvironment.make(
            base: ["PATH": "/usr/bin:/bin", "EXAMPLE": "base"],
            overrides: ["EXAMPLE": "plugin"]
        )

        #expect(environment["EXAMPLE"] == "plugin")
        #expect(environment["PATH"] == "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin")
    }

    @Test("plugin capabilities are reported as available instead of ambiguous on-demand permissions")
    func pluginCapabilityStatusIsExplicit() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {
              "schemaVersion":3,
              "name":"chatos-browser-cdp",
              "version":"0.1.4",
              "mcpServers":{"browser-cdp":{"type":"stdio","bin":"browser-cdp"}},
              "permissions":[
                {"permission":"browser.page.read","required":true},
                {"permission":"browser.network.observe","required":false}
              ]
            }
            """.utf8)
        )
        let record = NativeInstalledPluginRecord(
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "0.1.4",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: root.path,
            installedAt: "2026-08-27T00:00:00Z"
        )

        let permissions = NativePluginPermissionInspector.permissions(
            record: record,
            manifest: manifest
        )

        #expect(permissions.allSatisfy { $0.status == "ready" })
        #expect(permissions.allSatisfy { $0.statusLabel == "已可用" })
        #expect(permissions.allSatisfy { !$0.canRequest })
        #expect(permissions[1].label == "查看浏览器网络")
        #expect(permissions[1].summary.contains("WebSocket"))
    }

    @Test("older plugin launchers show a non-blocking unknown permission state")
    func oldPluginPermissionLauncherDoesNotStartMCP() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let launcher = bin.appendingPathComponent("open-computer-use")
        try Data("#!/bin/sh\nexit 99\n".utf8).write(to: launcher)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: launcher.path
        )
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {
              "schemaVersion":3,
              "name":"open-computer-use",
              "version":"0.8.1",
              "mcpServers":{"computer-use":{"type":"stdio","bin":"open-computer-use"}},
              "permissions":[
                {"permission":"process.spawn","required":true},
                {"permission":"computer.control","required":true}
              ]
            }
            """.utf8)
        )
        let record = NativeInstalledPluginRecord(
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "0.8.1",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: root.path,
            installedAt: "2026-08-27T00:00:00Z"
        )

        let permission = try #require(
            NativePluginPermissionInspector.permissions(record: record, manifest: manifest)
                .first(where: { $0.permissionID == "computer.screen-recording" })
        )

        #expect(permission.status == "unknown")
        #expect(permission.statusLabel == "等待检测")
    }

    private enum ComputerUseFixtureBehavior {
        case success
        case hang
    }

    private func makeComputerUseLeaseClient(
        root: URL,
        label: String,
        behavior: ComputerUseFixtureBehavior = .success
    ) async throws -> (NativePluginStdioClient, [NativeJSONValue], URL, URL, URL) {
        let directory = root.appendingPathComponent(label, isDirectory: true)
        let visual = directory.appendingPathComponent("visual", isDirectory: true)
        let artifacts = directory.appendingPathComponent("artifacts", isDirectory: true)
        let log = directory.appendingPathComponent("calls.log")
        try FileManager.default.createDirectory(at: visual, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: artifacts, withIntermediateDirectories: true)
        let script = directory.appendingPathComponent("fixture.zsh")
        let callResponse: String
        switch behavior {
        case .success:
            callResponse = """
                echo "{\\"jsonrpc\\":\\"2.0\\",\\"id\\":$id,\\"result\\":{\\"content\\":[{\\"type\\":\\"text\\",\\"text\\":\\"ok-\(label)\\"}]}}"
            """
        case .hang:
            callResponse = "true"
        }
        try """
        while IFS= read -r line; do
          if [[ "$line" == *'tools/list'* ]]; then
            echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"observe","inputSchema":{"type":"object"}}]}}'
          elif [[ "$line" == *'tools/call'* ]]; then
            echo call >> '\(log.path)'
            id=$(echo "$line" | sed -E 's/.*"id":([0-9]+).*/\\1/')
            \(callResponse)
          elif [[ "$line" == *'initialize'* ]]; then
            echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}}}}'
          fi
        done
        """.write(to: script, atomically: true, encoding: .utf8)
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {"schemaVersion":3,"name":"fixture","version":"1.0.0","mcpServers":{"computer-use":{"type":"stdio","bin":"fixture","args":[],"requiresExclusiveExecution":true}}}
            """.utf8)
        )
        let launch = NativePreparedPluginLaunch(
            manifest: manifest,
            componentKey: "computer-use",
            server: manifest.mcpServers["computer-use"]!,
            executableURL: URL(fileURLWithPath: "/bin/zsh"),
            arguments: [script.path],
            environment: [:],
            installationURL: directory,
            visualSessionURL: visual,
            artifactURL: artifacts,
            displayName: label
        )
        let client = NativePluginStdioClient(launch: launch)
        try await client.start()
        let initialized = try await client.initialize()
        return (client, initialized.tools, visual, artifacts, log)
    }

}
