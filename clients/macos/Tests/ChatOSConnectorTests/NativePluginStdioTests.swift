@testable import ChatOSConnector
import ChatOSCore
import Darwin
import Foundation
import Testing

private actor PluginProgressCollector {
    private var updates: [NativePluginProgressUpdate] = []

    func append(_ update: NativePluginProgressUpdate) {
        updates.append(update)
    }

    func snapshot() -> [NativePluginProgressUpdate] { updates }
}

extension NativePluginRuntimeTests {
    @Test("plugin progress presentation bounds untrusted message text")
    func pluginProgressPresentationBoundsMessageText() {
        let detail = NativePluginProgressPresentation.detail(for: .init(
            progress: 1,
            total: 2,
            message: "进" + String(repeating: "x", count: 5_000)
        ))

        #expect(detail.count == NativePluginProgressPresentation.maximumMessageCharacters + 1)
        #expect(detail.hasPrefix("进"))
        #expect(detail.hasSuffix("…"))
        #expect(NativePluginProgressPresentation.detail(for: .init(
            progress: 2,
            total: 5,
            message: "  \n"
        )) == "Progress 2/5")
        #expect(NativePluginProgressPresentation.detail(for: .init(
            progress: 1e300,
            total: nil,
            message: nil
        )) == "Progress ?")
    }

    @Test("stdio client negotiates and delivers MCP progress notifications")
    func stdioProgressNotifications() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let script = root.appendingPathComponent("fixture.zsh")
        try #"""
        while IFS= read -r line; do
          if [[ "$line" == *'tools/list'* ]]; then
            echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"work","inputSchema":{"type":"object"}}]}}'
          elif [[ "$line" == *'tools/call'* ]]; then
            token=$(print -r -- "$line" | /usr/bin/sed -E 's/.*"progressToken":"([^"]+)".*/\1/')
            echo "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\",\"params\":{\"progressToken\":\"$token\",\"progress\":2,\"total\":5,\"message\":\"action 2/5: click export\"}}"
            echo '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"ok"}]}}'
          elif [[ "$line" == *'initialize'* ]]; then
            echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}}}}'
          fi
        done
        """#.write(to: script, atomically: true, encoding: .utf8)
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data("""
            {"schemaVersion":3,"name":"fixture","version":"1.0.0","mcpServers":{"fixture":{"type":"stdio","bin":"fixture","args":[]}}}
            """.utf8)
        )
        let launch = NativePreparedPluginLaunch(
            record: try nativePluginTestRecord(installationURL: root),
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
        let collector = PluginProgressCollector()
        try await client.start()
        _ = try await client.initialize()
        _ = try await client.callTool(
            name: "work",
            arguments: .object([:]),
            timeout: .seconds(2),
            onProgress: { update in await collector.append(update) }
        )

        let updates = await collector.snapshot()
        #expect(updates == [.init(
            progress: 2,
            total: 5,
            message: "action 2/5: click export"
        )])
        await client.terminate()
    }

    @Test("stdio output buffer budget is bounded by bytes instead of pipe chunk count")
    func stdioOutputBufferUsesByteBudget() {
        let budget = NativePluginOutputBufferBudget(maximumBytes: 1_024)

        for _ in 0..<1_024 {
            #expect(budget.reserve(1))
        }
        #expect(budget.currentBytes == 1_024)
        #expect(!budget.reserve(1))

        budget.release(768)
        #expect(budget.currentBytes == 256)
        #expect(budget.reserve(768))
        #expect(!budget.reserve(1))
    }

    @Test("stdio plugin sandbox blocks undeclared user file reads")
    func stdioSandboxBlocksUndeclaredFileRead() async throws {
        let parent = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let installation = parent.appendingPathComponent("plugin", isDirectory: true)
        try FileManager.default.createDirectory(at: installation, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: parent) }
        let secret = parent.appendingPathComponent("host-secret.txt")
        try Data("must-not-be-readable".utf8).write(to: secret)
        let script = installation.appendingPathComponent("fixture.zsh")
        try """
        while IFS= read -r line; do
          if [[ "$line" == *'tools/list'* ]]; then
            echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"probe","inputSchema":{"type":"object"}}]}}'
          elif [[ "$line" == *'tools/call'* ]]; then
            if /bin/cat '(secret.path)' >/dev/null 2>&1; then
              echo '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"accessible"}]}}'
            else
              echo '{"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"blocked"}]}}'
            fi
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
            record: try nativePluginTestRecord(installationURL: installation),
            manifest: manifest,
            componentKey: "fixture",
            server: manifest.mcpServers["fixture"]!,
            executableURL: URL(fileURLWithPath: "/bin/zsh"),
            arguments: [script.path],
            environment: [:],
            installationURL: installation,
            visualSessionURL: installation.appendingPathComponent("visual"),
            artifactURL: installation.appendingPathComponent("artifacts"),
            displayName: "Sandbox fixture"
        )
        let client = NativePluginStdioClient(launch: launch)
        try await client.start()
        _ = try await client.initialize()
        let result = try await client.callTool(
            name: "probe",
            arguments: .object([:]),
            timeout: .seconds(2)
        )
        #expect(
            result.jsonObject?["content"]?.jsonArray?.first?
                .jsonObject?["text"]?.jsonString == "blocked"
        )
        await client.terminate()
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
            record: try nativePluginTestRecord(installationURL: root),
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

    @Test("terminating a stdio plugin also terminates its spawned descendants")
    func stdioTerminationKillsPluginProcessGroup() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let runtime = root.appendingPathComponent("runtime", isDirectory: true)
        try FileManager.default.createDirectory(at: runtime, withIntermediateDirectories: true)
        let grandchildPIDFile = runtime.appendingPathComponent("grandchild.pid")
        let pluginPIDFile = runtime.appendingPathComponent("plugin.pid")
        let script = root.appendingPathComponent("fixture.zsh")
        try """
        echo $$ > '\(pluginPIDFile.path)'
        while IFS= read -r line; do
          if [[ "$line" == *'tools/list'* ]]; then
            echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"hang","description":"Hang","inputSchema":{"type":"object"}}]}}'
          elif [[ "$line" == *'tools/call'* ]]; then
            /bin/sleep 60 &
            echo $! > '\(grandchildPIDFile.path)'
            wait
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
            record: try nativePluginTestRecord(installationURL: root),
            manifest: manifest,
            componentKey: "fixture",
            server: manifest.mcpServers["fixture"]!,
            executableURL: URL(fileURLWithPath: "/bin/zsh"),
            arguments: [script.path],
            environment: ["CHATOS_PLUGIN_DATA_DIR": runtime.path],
            installationURL: root,
            visualSessionURL: root.appendingPathComponent("visual"),
            artifactURL: root.appendingPathComponent("artifacts"),
            displayName: "Fixture"
        )
        let client = NativePluginStdioClient(launch: launch)
        try await client.start()
        _ = try await client.initialize()
        let call = Task {
            try await client.callTool(name: "hang", arguments: .object([:]), timeout: .seconds(30))
        }
        for _ in 0..<100 where !FileManager.default.fileExists(atPath: grandchildPIDFile.path) {
            try await Task.sleep(for: .milliseconds(20))
        }
        let text = try String(contentsOf: grandchildPIDFile, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let grandchildPID = try #require(pid_t(text))
        let pluginText = try String(contentsOf: pluginPIDFile, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let pluginPID = try #require(pid_t(pluginText))
        #expect(Darwin.kill(grandchildPID, 0) == 0)
        #expect(Darwin.kill(pluginPID, 0) == 0)

        await client.terminate()
        _ = try? await call.value
        for _ in 0..<100 where Darwin.kill(grandchildPID, 0) == 0 {
            try await Task.sleep(for: .milliseconds(20))
        }
        for _ in 0..<150 where Darwin.kill(pluginPID, 0) == 0 {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(Darwin.kill(grandchildPID, 0) == -1)
        #expect(errno == ESRCH)
        #expect(Darwin.kill(pluginPID, 0) == -1)
        #expect(errno == ESRCH)
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
            record: try nativePluginTestRecord(installationURL: root),
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
            record: try NativeBrowserPluginUsageFixture.installedRecord(at: installation),
            componentKey: NativeBrowserPluginIdentity.componentKey,
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
                arguments: NativeBrowserPluginUsageFixture.openArguments,
                timeout: .seconds(60)
            )
            #expect(opened.jsonObject?["isError"]?.jsonBool != true)

            let navigated = try await client.callTool(
                name: "browser_navigate",
                arguments: NativeBrowserPluginUsageFixture.navigationArguments,
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
            record: try nativePluginTestRecord(installationURL: root),
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

}
