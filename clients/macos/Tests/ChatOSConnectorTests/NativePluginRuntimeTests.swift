@testable import ChatOSConnector
import ChatOSCore
import Darwin
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
        test "$CHATOS_PLUGIN_FILE_WATCH_MODE" = "polling"
        mkdir "$CHATOS_PLUGIN_DATA_DIR.lock"
        rmdir "$CHATOS_PLUGIN_DATA_DIR.lock"
        mkdir "$CHATOS_PLUGIN_CACHE_DIR.lock"
        rmdir "$CHATOS_PLUGIN_CACHE_DIR.lock"
        sleep 60 &
        echo $! > "$CHATOS_PLUGIN_DATA_DIR/child.pid"
        exec node -e 'const fs=require("node:fs");const http=require("node:http");const watcher=fs.watch(process.env.CHATOS_PLUGIN_DATA_DIR,()=>{});const port=Number(process.env.CHATOS_PLUGIN_APP_PORT);const server=http.createServer((req,res)=>{res.writeHead(200,{"content-type":"text/html"});res.end(process.env.CHATOS_PLUGIN_RELEASE_ID)});server.on("close",()=>watcher.close());server.listen(port,"127.0.0.1")'
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
            installedAt: "2026-09-02T00:00:00Z",
            packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
                installationURL: installation,
                maximumFiles: 20_000,
                maximumBytes: 512 * 1_024 * 1_024
            )
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
        #expect(String(decoding: body, as: UTF8.self) == "release-demo")
        #expect(FileManager.default.fileExists(
            atPath: runtimeRoot.appendingPathComponent("data", isDirectory: true).path
        ))
        let childPID = try await waitForPID(below: runtimeRoot)
        #expect(processExists(childPID))

        var updatedRecord = record
        updatedRecord.releaseID = "release-demo-2"
        updatedRecord.artifactSHA256 = String(repeating: "b", count: 64)
        let updatedLaunch = try await runtime.launch(
            record: updatedRecord,
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
        let updatedBody = try await URLSession.shared.data(from: updatedLaunch.url).0
        #expect(String(decoding: updatedBody, as: UTF8.self) == "release-demo-2")
        await runtime.stopAll()
        #expect(!processExists(childPID))
    }

    func waitForPID(below root: URL) async throws -> pid_t {
        for _ in 0..<50 {
            let url = FileManager.default.enumerator(
                at: root,
                includingPropertiesForKeys: nil
            )?.compactMap { $0 as? URL }
                .first { $0.lastPathComponent == "child.pid" }
            if let url,
               let value = try? String(contentsOf: url, encoding: .utf8),
               let pid = pid_t(value.trimmingCharacters(in: .whitespacesAndNewlines)) {
                return pid
            }
            try await Task.sleep(for: .milliseconds(20))
        }
        throw CocoaError(.fileReadNoSuchFile)
    }

    func processExists(_ pid: pid_t) -> Bool {
        kill(pid, 0) == 0 || errno == EPERM
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
        #expect(mcp.environment["CHATOS_PROJECT_NAME"] == "Project One")
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

    @Test
    func browserSessionApprovalSummaryExplainsExistingChromeInsteadOfOnlyHashingArguments() {
        let summary = NativeLocalConnectorService.safeArgumentSummary(
            toolName: "browser_session_open",
            arguments: .object([
                "mode": .string("chrome_extension"),
                "session_name": .string("今日 AI 新闻"),
            ])
        )

        #expect(summary.contains("用户现有的 Google Chrome"))
        #expect(summary.contains("今日 AI 新闻"))
        #expect(summary.contains("原生标签组"))
        #expect(!summary.contains("内容摘要"))
    }

    @Test
    func browserSessionOpenBuildsPairedChromeExecutionRequestFromVerifiedState() {
        let arguments = NativeLocalConnectorService.browserSessionArguments(
            arguments: .object([
                "mode": .string("managed"),
                "headless": .bool(true),
                "persistent_profile": .bool(true),
            ]),
            contextBody: ["task_title": .string("今日 AI 新闻")],
            browserExtensionPaired: true
        )

        #expect(arguments.jsonObject?["mode"]?.jsonString == "chrome_extension")
        #expect(arguments.jsonObject?["headless"] == nil)
        #expect(arguments.jsonObject?["persistent_profile"] == nil)
        #expect(arguments.jsonObject?["session_name"]?.jsonString == "今日 AI 新闻")
    }

    @Test
    func browserSessionOpenBuildsManagedFallbackExecutionRequestWithoutPairing() {
        let arguments = NativeLocalConnectorService.browserSessionArguments(
            arguments: .object(["mode": .string("chrome_extension")]),
            contextBody: ["task_title": .string("首次使用")],
            browserExtensionPaired: false
        )

        #expect(arguments.jsonObject?["mode"]?.jsonString == "managed")
        #expect(arguments.jsonObject?["session_name"]?.jsonString == "首次使用")
    }

    @Test
    func browserSessionOpenInheritsTaskTitleForNativeChromeGroup() {
        let arguments = NativeLocalConnectorService.browserSessionArguments(
            arguments: .object(["mode": .string("chrome_extension")]),
            contextBody: [
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
            contextBody: ["task_title": .string("Ignored title")]
        )

        #expect(arguments.jsonObject?["session_name"]?.jsonString == "Explicit group")
    }

    @Test
    func browserSessionPermissionDescriptionStatesExistingChromeIsUsed() {
        let description = NativeLocalConnectorService.permissionDescription(
            toolName: "browser_session_open",
            requiredPermissions: ["browser.chrome.attach"]
        )

        #expect(description.contains("用户已配对的 Google Chrome"))
        #expect(description.contains("browser.chrome.attach"))
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

}
