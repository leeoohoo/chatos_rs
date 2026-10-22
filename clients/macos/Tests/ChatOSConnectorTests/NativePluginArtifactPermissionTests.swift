import ChatOSCore
@testable import ChatOSConnector
import Foundation
import XCTest

final class NativePluginArtifactPermissionTests: XCTestCase {
    func testBrowserManifestPermissionsPersistAndVerifyActualScreenshotBytes() throws {
        let repository = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data(contentsOf: repository.appendingPathComponent("plugins/browser/npm/chatos.plugin.json"))
        )
        let permissions = Set(manifest.permissions.map(\.permission))
        XCTAssertTrue(permissions.contains("artifact.create"))

        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let artifacts = root.appendingPathComponent("artifacts", isDirectory: true)
        let workspace = root.appendingPathComponent("workspace", isDirectory: true)
        try FileManager.default.createDirectory(at: artifacts, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: workspace, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let bytes = try XCTUnwrap(Data(base64Encoded:
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="))
        try bytes.write(to: artifacts.appendingPathComponent("capture.png"))
        let result = NativeJSONValue.object([
            "content": .array([]),
            "_meta": .object(["chatos/artifacts": .array([.object([
                "producer_artifact_id": .string("browser-capture"),
                "relative_path": .string("capture.png"),
                "display_name": .string("capture.png"),
                "media_type": .string("image/png"),
                "size_bytes": .number(Double(bytes.count)),
                "sha256": .string(NativePluginHash.sha256(bytes)),
            ])])]),
        ])
        let identity = NativePluginRuntimeStore.Identity(
            runID: "research-run", pluginID: "browser-plugin", releaseID: "fixture-release",
            version: manifest.version, artifactSHA256: String(repeating: "a", count: 64),
            componentKey: "browser-cdp-mcp", adapterSessionID: "browser-fixture"
        )
        func register(_ snapshot: Set<String>, workspaceID: String? = "workspace") throws -> NativeJSONValue {
            try NativePluginArtifactRegistrar.register(
                result: result, identity: identity, ownerUserID: "fixture-user", deviceID: "fixture-device",
                workspaceID: workspaceID, workspaceRootURL: workspace, artifactRootURL: artifacts,
                permissionSnapshot: snapshot, toolName: "browser_screenshot"
            )
        }

        // Reproduce the old installed manifest without granting permissions behind its back.
        XCTAssertThrowsError(try register(permissions.subtracting(["artifact.create"]))) { error in
            guard let error = error as? NativePluginRuntimeError else {
                return XCTFail("Expected a Plugin permission failure")
            }
            let message = NativeAgentPluginFailurePresenter.message(for: error, projectRootURLs: [root])
            XCTAssertTrue(message.contains("artifact.create"))
            XCTAssertTrue(message.contains("更新或重新安装"))
        }
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: workspace.path).isEmpty)

        // A workspace configuration failure must not be misdiagnosed as a missing permission.
        XCTAssertThrowsError(try register(permissions, workspaceID: nil)) { error in
            guard let error = error as? NativePluginRuntimeError else {
                return XCTFail("Expected a Plugin workspace failure")
            }
            let message = NativeAgentPluginFailurePresenter.message(for: error, projectRootURLs: [root])
            XCTAssertTrue(message.contains("工作区"))
            XCTAssertFalse(message.contains("artifact.create"))
        }

        let registered = try register(permissions)
        let descriptor = try XCTUnwrap(registered.jsonObject?["_meta"]?.jsonObject?["chatos/artifacts"]?
            .jsonArray?.first?.jsonObject?["artifact"]?.jsonObject)
        let path = try XCTUnwrap(descriptor["workspace_relative_path"]?.jsonString)
        let copied = try Data(contentsOf: workspace.appendingPathComponent(path))
        XCTAssertEqual(copied, bytes)
        XCTAssertEqual(descriptor["sha256"]?.jsonString, NativePluginHash.sha256(copied))
        XCTAssertEqual(descriptor["size_bytes"]?.jsonNumber, Double(bytes.count))
        let delivery = try XCTUnwrap(registered.jsonObject?["content"]?.jsonArray?.first?
            .jsonObject?["text"]?.jsonString)
        XCTAssertTrue(delivery.contains("[capture.png](\(path))"))
    }

    func testPermissionFailureKeepsCauseButRedactsPathsAndSecrets() {
        let message = NativeAgentPluginFailurePresenter.message(
            for: .permissionDenied("artifact.create denied at /tmp/private-project/capture.png api_key=secret-value"),
            projectRootURLs: [URL(fileURLWithPath: "/tmp/private-project")]
        )
        XCTAssertTrue(message.contains("artifact.create"))
        XCTAssertFalse(message.contains("/tmp/private-project"))
        XCTAssertFalse(message.contains("secret-value"))
        XCTAssertTrue(message.contains("[redacted]"))
    }
}
