@testable import ChatOSConnector
import ChatOSCore
import Darwin
import Foundation
import Testing

extension NativePluginRuntimeTests {
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

    @Test("visual bridges reject oversized base64 before frame decoding")
    func visualBridgeBase64IsBounded() {
        let browserResult = NativeJSONValue.object([
            "content": .array([
                .object([
                    "type": .string("image"),
                    "data": .string(String(
                        repeating: "A",
                        count: NativeBrowserVisualBridge.maximumEncodedCharacters + 1
                    )),
                ]),
            ]),
        ])
        let computerResult = NativeJSONValue.object([
            "content": .array([
                .object([
                    "type": .string("image"),
                    "mimeType": .string("image/png"),
                    "data": .string(String(
                        repeating: "A",
                        count: NativeComputerUseVisualBridge.maximumEncodedCharacters + 1
                    )),
                ]),
            ]),
        ])

        #expect(NativeBrowserVisualBridge.captureFrame(
            from: browserResult,
            artifactRootURL: FileManager.default.temporaryDirectory
        ) == nil)
        #expect(NativeComputerUseVisualBridge.captureFrame(from: computerResult) == nil)
    }

    @Test("browser visual bridge rejects non-PNG image payloads")
    func browserVisualBridgeRequiresPNGSignature() {
        let invalid = Data("not-a-png".utf8)
        let result = NativeJSONValue.object([
            "content": .array([
                .object([
                    "type": .string("image"),
                    "data": .string(invalid.base64EncodedString()),
                ]),
            ]),
        ])

        #expect(NativeBrowserVisualBridge.captureFrame(
            from: result,
            artifactRootURL: FileManager.default.temporaryDirectory
        ) == nil)
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

        let unchangedSessions = NativePluginVisualSessionReader.read(
            descriptors: [
                try descriptor(
                    adapterSessionID: "adapter-browser-unchanged",
                    componentKey: "browser-cdp",
                    taskTitle: "检查网站",
                    boundAt: Date(timeIntervalSince1970: 30)
                ),
            ],
            now: ISO8601DateFormatter().date(from: "2026-08-28T03:00:01Z")!,
            loadFrameDataForAdapterSessionIDs: ["adapter-browser-unchanged"],
            knownFrameSequencesByAdapterSessionID: ["adapter-browser-unchanged": 3]
        )
        #expect(unchangedSessions.count == 1)
        #expect(unchangedSessions.first?.frameData == nil)
    }

}
