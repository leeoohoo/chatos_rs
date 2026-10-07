@testable import ChatOSConnector
import ChatOSCore
import Darwin
import Foundation
import Testing

extension NativePluginRuntimeTests {
    @Test("plugin permission diagnostics drain oversized output without deadlocking")
    func pluginPermissionDiagnosticsBoundOversizedOutput() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let launcher = bin.appendingPathComponent("open-computer-use")
        try Data("""
        #!/bin/sh
        # check-permissions
        dd if=/dev/zero bs=1200000 count=1 2>/dev/null | tr '\\000' x
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
              "permissions": []
            }
            """.utf8)
        )
        let record = NativeInstalledPluginRecord(
            pluginID: "plugin-large-output",
            releaseID: "release-1",
            version: "0.8.1",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: root.path,
            installedAt: "2026-08-27T00:00:00Z",
            packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
                installationURL: root,
                maximumFiles: 20_000,
                maximumBytes: 512 * 1_024 * 1_024
            )
        )

        let clock = ContinuousClock()
        let startedAt = clock.now
        let permissions = await NativePluginPermissionInspector.permissions(
            record: record,
            manifest: manifest
        )

        #expect(clock.now - startedAt < .seconds(5))
        #expect(permissions.map(\.permissionID) == [
            "computer.accessibility",
            "computer.screen-recording",
        ])
        #expect(permissions.allSatisfy { $0.status == "unknown" })
    }

    @Test("timed out permission diagnostics terminate and reap their process group")
    func pluginPermissionDiagnosticsTimeoutCleansProcessGroup() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let launcher = bin.appendingPathComponent("open-computer-use")
        let diagnostic = root.deletingLastPathComponent()
            .appendingPathComponent("diagnostic-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: diagnostic, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: diagnostic) }
        try Data("""
        #!/bin/sh
        # check-permissions
        trap '' TERM
        printf '%s' "$$" > "$CHATOS_PLUGIN_DATA_DIR/launcher.pid"
        sleep 60 &
        printf '%s' "$!" > "$CHATOS_PLUGIN_DATA_DIR/child.pid"
        wait
        """.utf8).write(to: launcher)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: launcher.path
        )

        let clock = ContinuousClock()
        let startedAt = clock.now
        let launcherPIDURL = diagnostic.appendingPathComponent("launcher.pid")
        let childPIDURL = diagnostic.appendingPathComponent("child.pid")
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data(#"{"schemaVersion":3,"name":"open-computer-use","version":"1.0.0"}"#.utf8)
        )
        let record = try nativePluginTestRecord(
            installationURL: root,
            pluginID: "plugin-timeout"
        )
        do {
            _ = try await Task.detached {
                try NativePluginPermissionInspector.runLauncherBlocking(
                    record: record,
                    manifest: manifest,
                    command: "check-permissions",
                    timeout: 2,
                    diagnosticDirectory: diagnostic,
                    spawnObserver: { _ in
                        // The timeout intentionally tests a running process group. Under the
                        // full parallel suite, wait for the tiny launcher to create its child
                        // before starting the diagnostic deadline.
                        for _ in 0..<500
                            where !FileManager.default.fileExists(atPath: launcherPIDURL.path)
                                || !FileManager.default.fileExists(atPath: childPIDURL.path) {
                            Thread.sleep(forTimeInterval: 0.01)
                        }
                    }
                )
            }.value
            Issue.record("expected permission diagnostics to time out")
        } catch {
            #expect(error.localizedDescription.contains("Plugin 权限检测超时"))
        }
        #expect(clock.now - startedAt < .seconds(12))

        let launcherPIDText = try String(
            contentsOf: launcherPIDURL,
            encoding: .utf8
        )
        let childPIDText = try String(
            contentsOf: childPIDURL,
            encoding: .utf8
        )
        guard let launcherPID = pid_t(launcherPIDText),
              let childPID = pid_t(childPIDText) else {
            Issue.record("permission diagnostic process IDs were not valid")
            return
        }
        for _ in 0..<50 where processExists(launcherPID) || processExists(childPID) {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(!processExists(launcherPID))
        #expect(!processExists(childPID))
    }

    @Test("plugin processes receive a minimal environment without host secrets")
    func pluginProcessEnvironmentDropsHostSecrets() {
        let environment = NativePluginProcessEnvironment.make(
            base: [
                "PATH": "/private/bin:/usr/bin",
                "LANG": "zh_CN.UTF-8",
                "AWS_SECRET_ACCESS_KEY": "host-secret",
                "EXAMPLE": "host-value",
            ],
            overrides: [
                "EXAMPLE": "plugin",
                "PATH": "/attacker/bin",
                "CHATOS_PLUGIN_DATA_DIR": "/plugin/data",
            ]
        )

        #expect(environment["EXAMPLE"] == "plugin")
        #expect(environment["PATH"] == "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin")
        #expect(environment["LANG"] == "zh_CN.UTF-8")
        #expect(environment["HOME"] == "/plugin/data")
        #expect(environment["TMPDIR"] == nil)
        #expect(environment["AWS_SECRET_ACCESS_KEY"] == nil)
    }

    @Test("plugin temporary files stay inside the isolated cache root")
    func pluginProcessEnvironmentUsesIsolatedTemporaryDirectory() {
        let environment = NativePluginProcessEnvironment.make(
            base: ["TMPDIR": "/Users/example/shared-temp"],
            overrides: [
                "CHATOS_PLUGIN_DATA_DIR": "/plugin/data",
                "CHATOS_PLUGIN_CACHE_DIR": "/plugin/cache",
            ]
        )

        #expect(environment["HOME"] == "/plugin/data")
        #expect(environment["TMPDIR"] == "/plugin/cache")
    }

    @Test("host-selected plugin home overrides the isolated default without inheriting secrets")
    func pluginProcessEnvironmentUsesHostSelectedHome() {
        let environment = NativePluginProcessEnvironment.make(
            base: [
                "HOME": "/Users/example",
                "AWS_SECRET_ACCESS_KEY": "host-secret",
            ],
            overrides: ["CHATOS_PLUGIN_DATA_DIR": "/plugin/data"],
            homeDirectory: "/Users/example"
        )

        #expect(environment["HOME"] == "/Users/example")
        #expect(environment["CHATOS_PLUGIN_DATA_DIR"] == "/plugin/data")
        #expect(environment["AWS_SECRET_ACCESS_KEY"] == nil)
    }

    @Test("Browser CDP receives only the loopback and native bridge exceptions it needs")
    func browserPluginLaunchUsesNarrowBridgeExceptions() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let installation = root.appendingPathComponent("plugin", isDirectory: true)
        let executable = installation
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent("chatos-browser-cdp")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(
            at: executable.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("#!/bin/sh\n".utf8).write(to: executable)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: executable.path
        )
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {
              "schemaVersion":3,
              "name":"chatos-browser-cdp",
              "version":"0.1.15",
              "mcpServers":{
                "browser-cdp-mcp":{
                  "type":"stdio",
                  "bin":"chatos-browser-cdp",
                  "args":["mcp"]
                }
              },
              "permissions":[
                {"permission":"process.spawn","required":true},
                {"permission":"browser.chrome.attach","required":false}
              ]
            }
            """.utf8)
        )
        let record = NativeInstalledPluginRecord(
            pluginID: "browser-plugin",
            releaseID: "release-1",
            version: "0.1.15",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: installation.path,
            installedAt: "2026-10-07T00:00:00Z",
            pluginKey: NativeBrowserPluginIdentity.marketplaceKey,
            packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
                installationURL: installation,
                maximumFiles: 100,
                maximumBytes: 1_024 * 1_024
            )
        )
        let prepared = try NativePluginManifestLoader.prepare(
            record: record,
            manifest: manifest,
            componentKey: NativeBrowserPluginIdentity.componentKey,
            serverKey: nil,
            adapterSessionID: "browser-session",
            ownerUserID: "user-1",
            deviceID: "device-1",
            workspaceRoot: nil,
            permissionSnapshot: ["process.spawn", "browser.chrome.attach"],
            runtimeRootURL: root.appendingPathComponent("runtime", isDirectory: true)
        )

        let launch = NativePluginProcessLaunch(stdio: prepared)
        #expect(launch.networkAccess == .loopbackServer)
        #expect(launch.homeDirectory == FileManager.default.homeDirectoryForCurrentUser)
        #expect(launch.writableDirectories.contains(where: {
            $0.path.hasSuffix("Library/Application Support/Chatos/browser-bridge")
        }))
        #expect(launch.writableDirectories.contains(where: {
            $0.path.hasSuffix("Google/Chrome/NativeMessagingHosts")
        }))
        let sandboxArguments = try NativePluginSandboxProfile.arguments(for: launch)
        let sandboxProfile = try #require(sandboxArguments.dropFirst().first)
        for browser in launch.executableDirectories {
            #expect(sandboxProfile.contains("(allow file-read* (subpath \"\(browser.path)\"))"))
            #expect(sandboxProfile.contains("(allow process-exec (subpath \"\(browser.path)\"))"))
        }
    }

    @Test("Computer Use receives only the macOS UI services required by its declared permission")
    func computerUseSandboxIncludesRequiredUIServices() throws {
        let installation = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: installation, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: installation) }
        let record = try nativePluginTestRecord(
            installationURL: installation,
            pluginID: "computer-use-plugin"
        )
        let permitted = NativePluginProcessLaunch(
            record: record,
            executableURL: URL(fileURLWithPath: "/bin/sh"),
            arguments: [],
            environment: [:],
            installationURL: installation,
            writableDirectories: [],
            workspaceRoot: nil,
            permissionSnapshot: ["process.spawn", "computer.control"],
            networkAccess: .disabled
        )
        let permittedProfile = try #require(
            NativePluginSandboxProfile.arguments(for: permitted).dropFirst().first
        )
        #expect(permittedProfile.contains("(allow user-preference-read)"))
        #expect(permittedProfile.contains("com.apple.CARenderServer"))
        #expect(permittedProfile.contains("com.apple.coreservices.launchservicesd"))
        #expect(permittedProfile.contains("com.apple.dock.fullscreen"))
        #expect(permittedProfile.contains("com.apple.replayd"))
        #expect(permittedProfile.contains("IOSurfaceRootUserClient"))
        #expect(!permittedProfile.contains("IOGPUDeviceUserClient"))
        #expect(permittedProfile.contains("com[.]apple[.]pasteboard"))
        #expect(permittedProfile.contains("com[.]apple[.]distributed_notifications"))

        var denied = permitted
        denied.permissionSnapshot = ["process.spawn"]
        let deniedProfile = try #require(
            NativePluginSandboxProfile.arguments(for: denied).dropFirst().first
        )
        #expect(!deniedProfile.contains("(allow user-preference-read)"))
        #expect(!deniedProfile.contains("com.apple.CARenderServer"))
        #expect(!deniedProfile.contains("com.apple.replayd"))
        #expect(!deniedProfile.contains("IOSurfaceRootUserClient"))
        #expect(!deniedProfile.contains("com[.]apple[.]pasteboard"))
    }

    @Test("Web Design receives isolated temporary storage and managed browser access")
    func webDesignPluginLaunchSupportsVisualVerification() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let installation = root.appendingPathComponent("plugin", isDirectory: true)
        let executable = installation
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent("chatos-web-design-studio")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(
            at: executable.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("#!/bin/sh\n".utf8).write(to: executable)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: executable.path
        )
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {
              "schemaVersion":3,
              "name":"chatos-web-design-studio",
              "version":"3.0.25",
              "mcpServers":{
                "web-design-mcp":{
                  "type":"stdio",
                  "bin":"chatos-web-design-studio",
                  "args":["mcp"]
                }
              },
              "permissions":[
                {"permission":"process.spawn","required":true},
                {"permission":"artifact.create","required":true}
              ],
              "runtimeContext":{
                "scope":"project",
                "components":["web-design-mcp"],
                "required":[],
                "optional":["project.id","workspace.id","workspace.root"],
                "storageIsolation":"project",
                "missingContext":"device"
              }
            }
            """.utf8)
        )
        let record = NativeInstalledPluginRecord(
            pluginID: "web-design-plugin",
            releaseID: "release-1",
            version: "3.0.25",
            artifactSHA256: String(repeating: "b", count: 64),
            installationPath: installation.path,
            installedAt: "2026-10-07T00:00:00Z",
            pluginKey: NativeWebDesignPluginIdentity.marketplaceKey,
            packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
                installationURL: installation,
                maximumFiles: 100,
                maximumBytes: 1_024 * 1_024
            )
        )
        let prepared = try NativePluginManifestLoader.prepare(
            record: record,
            manifest: manifest,
            componentKey: NativeWebDesignPluginIdentity.componentKey,
            serverKey: nil,
            adapterSessionID: "web-design-session",
            ownerUserID: "user-1",
            deviceID: "device-1",
            workspaceID: "workspace-1",
            workspaceRoot: root.appendingPathComponent("workspace", isDirectory: true),
            projectID: "project-1",
            permissionSnapshot: ["process.spawn", "artifact.create"],
            runtimeRootURL: root.appendingPathComponent("runtime", isDirectory: true)
        )

        let launch = NativePluginProcessLaunch(stdio: prepared)
        #expect(launch.networkAccess == .loopbackServer)
        #expect(launch.homeDirectory == nil)
        let dataDirectory = try #require(prepared.environment["CHATOS_PLUGIN_DATA_DIR"])
        let cacheDirectory = try #require(prepared.environment["CHATOS_PLUGIN_CACHE_DIR"])
        #expect(launch.writablePaths.map(\.path).contains(dataDirectory + ".lock"))
        #expect(launch.writablePaths.map(\.path).contains(cacheDirectory + ".lock"))
        let process = try NativePluginProcessLauncher.prepare(launch)
        #expect(process.environment["HOME"] == prepared.environment["CHATOS_PLUGIN_DATA_DIR"])
        #expect(process.environment["TMPDIR"] == prepared.environment["CHATOS_PLUGIN_CACHE_DIR"])
        #expect(process.environment["CHATOS_PLUGIN_SANDBOX"] == "macos-seatbelt")
        #expect(launch.managedBrowserAccess)
        let sandboxProfile = try #require(process.arguments.dropFirst(2).first)
        #expect(sandboxProfile.contains(dataDirectory + ".lock"))
        #expect(sandboxProfile.contains(cacheDirectory + ".lock"))
        #expect(sandboxProfile.contains("(allow iokit-open)"))
        #expect(sandboxProfile.contains("(allow network* (local unix-socket))"))
        #expect(sandboxProfile.contains("MachPortRendezvousServer"))
        #expect(sandboxProfile.contains("com[.]google[.]Chrome"))
        #expect(!sandboxProfile.contains("com.apple.tccd"))
        #expect(!sandboxProfile.contains("com.apple.replayd"))
        let temporaryRoot = NativePluginSandboxProfile.canonicalPath(
            FileManager.default.temporaryDirectory.path
        )
        let escapedTemporaryRoot = NSRegularExpression.escapedPattern(for: temporaryRoot)
            .replacingOccurrences(of: "\"", with: "\\\"")
        #expect(sandboxProfile.contains(
            "^\(escapedTemporaryRoot)/(com[.]google[.]Chrome|org[.]chromium[.]Chromium|com[.]microsoft[.]Edge)"
        ))
        #expect(!sandboxProfile.contains("(subpath \"\(temporaryRoot)\")"))
    }

    @Test("Document Tools may launch an installed browser for offline visual verification")
    func documentPluginLaunchSupportsVisualVerification() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let installation = root.appendingPathComponent("plugin", isDirectory: true)
        let executable = installation
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent("chatos-document-mcp")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(
            at: executable.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("#!/bin/sh\n".utf8).write(to: executable)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: executable.path
        )
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {
              "schemaVersion":3,
              "name":"chatos-document-mcp",
              "version":"0.1.7",
              "mcpServers":{
                "document-mcp":{
                  "type":"stdio",
                  "bin":"chatos-document-mcp",
                  "args":["mcp"]
                }
              },
              "permissions":[
                {"permission":"process.spawn","required":true},
                {"permission":"artifact.create","required":true}
              ]
            }
            """.utf8)
        )
        let record = NativeInstalledPluginRecord(
            pluginID: "document-plugin",
            releaseID: "release-1",
            version: "0.1.7",
            artifactSHA256: String(repeating: "c", count: 64),
            installationPath: installation.path,
            installedAt: "2026-10-07T00:00:00Z",
            pluginKey: NativeDocumentPluginIdentity.marketplaceKey,
            packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
                installationURL: installation,
                maximumFiles: 100,
                maximumBytes: 1_024 * 1_024
            )
        )
        let prepared = try NativePluginManifestLoader.prepare(
            record: record,
            manifest: manifest,
            componentKey: NativeDocumentPluginIdentity.componentKey,
            serverKey: nil,
            adapterSessionID: "document-session",
            ownerUserID: "user-1",
            deviceID: "device-1",
            workspaceRoot: nil,
            permissionSnapshot: ["process.spawn", "artifact.create"],
            runtimeRootURL: root.appendingPathComponent("runtime", isDirectory: true)
        )

        let launch = NativePluginProcessLaunch(stdio: prepared)
        #expect(launch.networkAccess == .disabled)
        #expect(launch.homeDirectory == nil)
        let installedBrowserPaths = [
            "/Applications/Google Chrome.app",
            "/Applications/Chromium.app",
            "/Applications/Microsoft Edge.app",
        ].filter(FileManager.default.fileExists(atPath:))
        #expect(Set(launch.executableDirectories.map(\.path)).isSuperset(of: installedBrowserPaths))
        let process = try NativePluginProcessLauncher.prepare(launch)
        #expect(process.environment["TMPDIR"] == prepared.environment["CHATOS_PLUGIN_CACHE_DIR"])
    }

    @Test("plugin capabilities are reported as available instead of ambiguous on-demand permissions")
    func pluginCapabilityStatusIsExplicit() async throws {
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
            installedAt: "2026-08-27T00:00:00Z",
            packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
                installationURL: root,
                maximumFiles: 20_000,
                maximumBytes: 512 * 1_024 * 1_024
            )
        )

        let permissions = await NativePluginPermissionInspector.permissions(
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
    func oldPluginPermissionLauncherDoesNotStartMCP() async throws {
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
            installedAt: "2026-08-27T00:00:00Z",
            packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
                installationURL: root,
                maximumFiles: 20_000,
                maximumBytes: 512 * 1_024 * 1_024
            )
        )

        let permission = try #require(
            await NativePluginPermissionInspector.permissions(record: record, manifest: manifest)
                .first(where: { $0.permissionID == "computer.screen-recording" })
        )

        #expect(permission.status == "unknown")
        #expect(permission.statusLabel == "等待检测")
    }

    enum ComputerUseFixtureBehavior {
        case success
        case hang
    }

    func makeComputerUseLeaseClient(
        root: URL,
        label: String,
        behavior: ComputerUseFixtureBehavior = .success
    ) async throws -> (NativePluginStdioClient, [NativeJSONValue], URL, URL, URL) {
        let directory = root.appendingPathComponent(label, isDirectory: true)
        let visual = directory.appendingPathComponent("visual", isDirectory: true)
        let artifacts = directory.appendingPathComponent("artifacts", isDirectory: true)
        let log = artifacts.appendingPathComponent("calls.log")
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
            record: try nativePluginTestRecord(installationURL: directory),
            manifest: manifest,
            componentKey: "computer-use",
            server: manifest.mcpServers["computer-use"]!,
            executableURL: URL(fileURLWithPath: "/bin/zsh"),
            arguments: [script.path],
            environment: [
                "CHATOS_PLUGIN_VISUAL_SESSION_DIR": visual.path,
                "CHATOS_PLUGIN_ARTIFACT_DIR": artifacts.path,
            ],
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
