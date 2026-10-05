@testable import ChatOSConnector
import ChatOSCore
import Darwin
import Foundation
import Testing

extension NativePluginRuntimeTests {
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

    @Test("visual session lifecycle changes wake subscribers")
    func visualSessionLifecycleChangesWakeSubscribers() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let fixture = try await makeComputerUseLeaseClient(root: root, label: "visual-events")
        let store = NativePluginRuntimeStore()
        let changes = await store.visualSessionChanges()
        var iterator = changes.makeAsyncIterator()
        await store.insert(
            identity: .init(
                runID: "run-visual-events",
                pluginID: "plugin-1",
                releaseID: "release-1",
                version: "1.0.0",
                artifactSHA256: String(repeating: "a", count: 64),
                componentKey: "desktop-control",
                adapterSessionID: "adapter-visual-events"
            ),
            client: fixture.0,
            tools: fixture.1,
            permissionSnapshot: [],
            displayName: "Visual events",
            visualSessionURL: fixture.2,
            artifactURL: fixture.3,
            projectRootURL: root,
            workspaceID: "workspace-1"
        )
        #expect(await iterator.next() != nil)

        await store.bindOwner(
            .init(
                conversationID: "conversation-1",
                sourceUserMessageID: "message-1",
                taskID: "task-1",
                taskRunID: "run-visual-events",
                taskTitle: "Visual events"
            ),
            adapterSessionID: "adapter-visual-events"
        )
        #expect(await iterator.next() != nil)

        #expect(await store.cancel(
            adapterSessionID: "adapter-visual-events",
            invocationID: nil
        ) == "cancelled")
        #expect(await iterator.next() != nil)
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
                // Keep a wide margin over the observation window. The full Connector suite runs
                // many process-heavy tests in parallel, so a 150 ms deadline could expire before
                // the test task resumed from its 60 ms sleep and falsely report a lease breach.
                timeout: .seconds(2)
            )
        }
        try await waitForTestFile(at: first.4)
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
    func pluginPermissionDiagnostics() async throws {
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

        let permissions = await NativePluginPermissionInspector.permissions(
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

    @Test("plugin permission snapshots merge concurrent diagnostics and allow forced refresh")
    func pluginPermissionSnapshotMergesConcurrentDiagnostics() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let bin = root.appendingPathComponent("bin", isDirectory: true)
        try FileManager.default.createDirectory(at: bin, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let launcher = bin.appendingPathComponent("open-computer-use")
        try Data("""
        #!/bin/sh
        # check-permissions
        printf 'x\\n' >> "$PWD/invocations.log"
        sleep 0.2
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
              "permissions": []
            }
            """.utf8)
        )
        let record = NativeInstalledPluginRecord(
            pluginID: "plugin-permission-cache",
            releaseID: "release-1",
            version: "0.8.1",
            artifactSHA256: String(repeating: "c", count: 64),
            installationPath: root.path,
            installedAt: "2026-08-27T00:00:00Z"
        )
        let cache = NativePluginPermissionSnapshotCache()

        async let first = cache.permissions(record: record, manifest: manifest)
        async let second = cache.permissions(record: record, manifest: manifest)
        let (firstPermissions, secondPermissions) = try await (first, second)

        #expect(firstPermissions == secondPermissions)
        #expect(try permissionInvocationCount(at: root) == 1)

        let cached = try await cache.permissions(record: record, manifest: manifest)
        #expect(cached == firstPermissions)
        #expect(try permissionInvocationCount(at: root) == 1)

        let refreshed = try await cache.permissions(
            record: record,
            manifest: manifest,
            forceRefresh: true
        )
        #expect(refreshed == firstPermissions)
        #expect(try permissionInvocationCount(at: root) == 2)
    }

    func permissionInvocationCount(at root: URL) throws -> Int {
        let contents = try String(
            contentsOf: root.appendingPathComponent("invocations.log"),
            encoding: .utf8
        )
        return contents.split(whereSeparator: \.isNewline).count
    }

}
