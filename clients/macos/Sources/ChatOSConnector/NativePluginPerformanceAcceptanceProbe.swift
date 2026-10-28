import ChatOSCore
import ChatOSNetworking
import Foundation

public enum NativePluginPerformanceAcceptanceProbeError: Error {
    case invalidFixture
    case unhealthyResponse
}

/// Launches one fully supervised local-HTTP Plugin Application from an isolated fixture.
/// The App enables this only behind its strict performance-acceptance environment gates.
public actor NativePluginPerformanceAcceptanceProbe {
    private let rootURL: URL
    private nonisolated let runtime: NativePluginApplicationRuntime
    private var launch: LocalConnectorPluginApplicationLaunch?

    public init(rootURL: URL) {
        self.rootURL = rootURL.standardizedFileURL
        runtime = NativePluginApplicationRuntime(
            processStateURL: rootURL.appendingPathComponent("processes.json"),
            pluginInstallationRootURL: rootURL.appendingPathComponent(
                "installation",
                isDirectory: true
            )
        )
    }

    public func start(ownerUserID: String) async throws {
        if launch != nil { return }
        let installation = rootURL.appendingPathComponent("installation", isDirectory: true)
        let binDirectory = installation.appendingPathComponent("bin", isDirectory: true)
        let uiDirectory = installation.appendingPathComponent("ui", isDirectory: true)
        let runtimeRoot = rootURL.appendingPathComponent("runtime", isDirectory: true)
        try FileManager.default.createDirectory(
            at: binDirectory,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try FileManager.default.createDirectory(
            at: uiDirectory,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        try Data(
            #"{"name":"chatos-acceptance-plugin","version":"1.0.0","bin":{"chatos-acceptance-plugin":"bin/chatos-acceptance-plugin"}}"#.utf8
        ).write(to: installation.appendingPathComponent("package.json"), options: .atomic)
        try Data("<html><body>ChatOS acceptance Plugin</body></html>".utf8).write(
            to: uiDirectory.appendingPathComponent("index.html"),
            options: .atomic
        )
        let launcher = binDirectory.appendingPathComponent("chatos-acceptance-plugin")
        let script = #"""
        #!/bin/sh
        exec /usr/local/bin/node -e '
        const http = require("node:http");
        const body = "chatos-performance-acceptance-plugin";
        http.createServer((request, response) => {
          response.writeHead(200, {
            "content-type": "text/plain",
            "content-length": Buffer.byteLength(body)
          });
          response.end(body);
        }).listen(Number(process.env.CHATOS_PLUGIN_APP_PORT), "127.0.0.1");
        '
        """#
        try Data(script.utf8).write(to: launcher, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: launcher.path
        )

        let manifest = try JSONDecoder().decode(NativePluginManifest.self, from: Data(#"""
        {
          "schemaVersion": 3,
          "name": "chatos-acceptance-plugin",
          "version": "1.0.0",
          "description": "Isolated performance acceptance Plugin",
          "ui": [{
            "componentKey": "acceptance",
            "source": "./ui/index.html",
            "title": "Acceptance",
            "surface": "workbench",
            "runtime": {
              "type": "local_http",
              "bin": "chatos-acceptance-plugin",
              "healthPath": "/",
              "launchTimeoutMs": 5000
            }
          }],
          "permissions": [{
            "permission": "process.spawn",
            "required": true,
            "components": ["acceptance"]
          }]
        }
        """#.utf8))
        guard let contribution = manifest.ui.first else {
            throw NativePluginPerformanceAcceptanceProbeError.invalidFixture
        }
        let record = NativeInstalledPluginRecord(
            pluginID: "chatos-performance-acceptance-plugin",
            releaseID: "chatos-performance-acceptance-release",
            version: "1.0.0",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: installation.path,
            installedAt: "2026-10-28T00:00:00Z",
            packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
                installationURL: installation,
                maximumFiles: 32,
                maximumBytes: 1 * 1_024 * 1_024
            )
        )
        let application = LocalConnectorPluginApplication(
            pluginID: record.pluginID,
            componentKey: contribution.componentKey,
            displayName: "Acceptance Plugin",
            description: "Isolated performance acceptance Plugin",
            requiresLocalRuntime: true
        )
        launch = try await runtime.launch(
            record: record,
            manifest: manifest,
            contribution: contribution,
            runtimeRootURL: runtimeRoot,
            application: application,
            hostContext: .init(
                ownerUserID: ownerUserID,
                deviceID: "performance-acceptance-device",
                workspaceID: nil,
                workspaceRoot: nil,
                projectID: nil,
                projectName: nil
            )
        )
        try await exercise()
    }

    public func exercise() async throws {
        guard let launch else {
            throw NativePluginPerformanceAcceptanceProbeError.invalidFixture
        }
        var request = URLRequest(url: launch.url)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 2
        let (data, response) = try await BoundedURLSessionDataLoader.load(
            request: request,
            maximumBytes: 4 * 1_024
        )
        guard (response as? HTTPURLResponse)?.statusCode == 200,
              data == Data("chatos-performance-acceptance-plugin".utf8) else {
            throw NativePluginPerformanceAcceptanceProbeError.unhealthyResponse
        }
    }

    public func stop() async {
        launch = nil
        await runtime.stopAll()
    }

    public nonisolated func terminateAllSynchronously() {
        runtime.terminateAllSynchronously()
    }
}
