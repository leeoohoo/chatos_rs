@testable import ChatOSConnector
import ChatOSCore
import Darwin
import Foundation
import Testing

extension NativePluginRuntimeTests {
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

    @Test("a timed-out Browser CDP tool keeps the MCP process available for recovery")
    func browserToolTimeoutKeepsPluginSessionAlive() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let script = root.appendingPathComponent("fixture.zsh")
        try """
        while IFS= read -r line; do
          if [[ "$line" == *'tools/list'* ]]; then
            echo '{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"browser_wait"},{"name":"browser_session_status"}]}}'
          elif [[ "$line" == *'"name":"browser_wait"'* ]]; then
            : # Deliberately leave the request pending until the host cancels it.
          elif [[ "$line" == *'"name":"browser_session_status"'* ]]; then
            echo '{"jsonrpc":"2.0","id":4,"result":{"content":[{"type":"text","text":"open"}],"structuredContent":{"state":"open"}}}'
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
            visualSessionURL: root.appendingPathComponent("visual"),
            artifactURL: root.appendingPathComponent("artifacts"),
            displayName: "Browser fixture"
        )
        let client = NativePluginStdioClient(launch: launch)
        try await client.start()
        let initialized = try await client.initialize()
        let store = NativePluginRuntimeStore()
        let identity = NativePluginRuntimeStore.Identity(
            runID: "run-1",
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "1.0.0",
            artifactSHA256: String(repeating: "a", count: 64),
            componentKey: "browser-cdp",
            adapterSessionID: "adapter-1"
        )
        await store.insert(
            identity: identity,
            client: client,
            tools: initialized.tools,
            permissionSnapshot: [],
            displayName: "Browser fixture",
            visualSessionURL: launch.visualSessionURL,
            artifactURL: launch.artifactURL,
            projectRootURL: nil,
            workspaceID: nil
        )

        do {
            _ = try await store.call(
                adapterSessionID: identity.adapterSessionID,
                invocationID: "wait-1",
                toolName: "browser_wait",
                arguments: .object([:]),
                timeout: .milliseconds(100)
            )
            Issue.record("Expected the browser call to time out")
        } catch let error as NativePluginRuntimeError {
            #expect(error.errorDescription == NativePluginRuntimeError.timeout.errorDescription)
        }

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

}
