import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeProgramMCPInstallationTests {
    @Test("an installed executable needs no manifest, keeps argument boundaries, and survives source removal")
    func localProgramLifecycle() async throws {
        let root = temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("user's $(literal) program")
        try writeExecutable(source)
        let service = try service(root)
        let draft = LocalConnectorPluginSourceDraft(
            kind: .localExecutable, location: source.path, exactVersion: "1.0.0",
            program: .init(displayName: "Local fixture", componentKey: "fixture", executablePath: source.path,
                           arguments: ["mcp", "a b", "$(touch nope)"], accessProfile: .restricted)
        )
        let parsed = try NativeLocalAgentPluginInstallerToolExecutor.pluginSource(
            try #require(NativePluginInstallerMetadata.source(draft).objectValue)
        )
        #expect(parsed == draft)
        let inspection = try await service.inspectPluginSource(draft)
        #expect(inspection.components.first?.toolNames == ["fixture_info"])
        try await service.installPluginSource(draft, expectedFingerprint: inspection.fingerprint)
        let record = try #require(await service.installerPluginRecords().values.first)
        #expect(record.source == .programMCP)
        #expect(record.sourceDraft == draft)
        try FileManager.default.removeItem(at: source)
        let result = try await callInfo(record: record, runtime: root.appendingPathComponent("runtime"))
        #expect(result.jsonObject?["isError"]?.jsonBool != true)
        let discovered = try await service.installedAgentPlugins(ownerUserID: "program-owner")
        #expect(discovered.contains { $0.pluginID == record.pluginID && $0.componentKey == "fixture" })
        let catalog = await service.customInstalledPlugins(forceRefresh: true)
        let visible = try #require(catalog.first { $0.pluginID == record.pluginID })
        #expect(visible.source == .programMCP)
        #expect(visible.runtimeStatus == .ready)
        try await service.uninstallPlugin(id: record.pluginID)
        #expect(await service.installerPluginRecords().isEmpty)
        #expect(!FileManager.default.fileExists(atPath: record.installationPath))
    }

    @Test("release programs bind official origin, downloaded bytes, actual MCP tools and in-place updates")
    func releaseLifecycle() async throws {
        let root = temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("source/tool's $(literal)")
        try writeExecutable(executable)
        let archive = try await makeArchive(source: executable.deletingLastPathComponent(), destination: root.appendingPathComponent("fixture.tgz"))
        let client = fixtureClient(archive: archive)
        let resolved = try await client.resolveArchive(repositoryURL: "https://github.com/test/program.git", tag: "v1.0.0", assetName: "program-macos-universal.tgz")
        #expect(resolved.archiveEntries.contains("./tool's $(literal)"))
        let service = try service(root, client: client)
        var source = LocalConnectorPluginSourceDraft(
            kind: .githubRelease, location: "https://github.com/test/program", exactVersion: "v1.0.0",
            program: .init(displayName: "Release fixture", componentKey: "fixture", executablePath: "tool's $(literal)",
                           arguments: ["mcp", "a b", "$(touch nope)"], accessProfile: .restricted,
                           assetName: "program-macos-universal.tgz", archiveSHA256: resolved.archiveSHA256)
        )
        var request = try #require(NativePluginInstallerMetadata.source(source).objectValue)
        request["replacing_plugin_id"] = .null
        let newSource = try NativeLocalAgentPluginInstallerToolExecutor.pluginSource(request)
        let replacement = try NativeLocalAgentPluginInstallerToolExecutor.replacementPluginID(request)
        let inspected = try await service.inspectPluginSource(newSource, replacingPluginID: replacement)
        #expect(NativeLocalAgentPluginInstallerToolExecutor.inspectionResult(
            inspected, source: newSource, replacingID: replacement
        ).objectValue?["replacing_plugin_id"] == .null)
        _ = try await service.installPluginSource(newSource, expectedFingerprint: inspected.fingerprint, replacingPluginID: replacement)
        let original = try #require(await service.installerPluginRecords().values.first)
        try await service.updatePluginEnabled(id: original.pluginID, enabled: false)
        source.exactVersion = "v2.0.0"
        let update = try await service.inspectPluginSource(source, replacingPluginID: original.pluginID)
        #expect(NativeLocalAgentPluginInstallerToolExecutor.inspectionResult(
            update, source: source, replacingID: original.pluginID
        ).objectValue?["replacing_plugin_id"] == .string(original.pluginID))
        try await service.installPluginSource(source, expectedFingerprint: update.fingerprint, replacingPluginID: original.pluginID)
        let record = try #require(await service.installerPluginRecord(id: original.pluginID))
        #expect(record.version == "v2.0.0")
        #expect(await service.installerPluginRecords().count == 1)
        #expect(await service.installerPluginInfo(record)["enabled"] == .bool(false))
        #expect(!FileManager.default.fileExists(atPath: original.installationPath))
        let persisted = try NativeConnectorStateStore(stateURL: root.appendingPathComponent("state.json")).load()
        #expect(persisted.installedPluginRecords?[record.pluginID]?.sourceDraft == source)
        #expect(try await callInfo(record: record, runtime: root.appendingPathComponent("runtime")).jsonObject?["isError"]?.jsonBool != true)
        source.program?.archiveSHA256 = String(repeating: "0", count: 64)
        await #expect(throws: NativeConnectorError.self) {
            _ = try await service.inspectPluginSource(source, replacingPluginID: record.pluginID)
        }
        #expect(await service.installerPluginRecord(id: record.pluginID)?.version == "v2.0.0")
    }

    @Test("changed executable after preflight cannot be installed")
    func changedLocalExecutable() async throws {
        let root = temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("fixture")
        try writeExecutable(executable)
        let service = try service(root)
        let draft = LocalConnectorPluginSourceDraft(kind: .localExecutable, location: executable.path, exactVersion: "1.0.0",
            program: .init(displayName: "fixture", componentKey: "fixture", executablePath: executable.path,
                           arguments: ["mcp", "a b", "$(touch nope)"], accessProfile: .restricted))
        let inspection = try await service.inspectPluginSource(draft)
        let handle = try FileHandle(forWritingTo: executable)
        try handle.seekToEnd()
        try handle.write(contentsOf: Data("\n// changed\n".utf8))
        try handle.close()
        await #expect(throws: NativeConnectorError.self) {
            try await service.installPluginSource(draft, expectedFingerprint: inspection.fingerprint)
        }
        #expect(await service.installerPluginRecords().isEmpty)
    }

    @Test("release archives reject symlinks before extraction and require an official origin")
    func rejectsUnsafeArchiveAndOrigin() async throws {
        let root = temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let source = root.appendingPathComponent("source")
        try FileManager.default.createDirectory(at: source, withIntermediateDirectories: true)
        try FileManager.default.createSymbolicLink(atPath: source.appendingPathComponent("escape").path, withDestinationPath: "/etc/passwd")
        let archive = try await makeArchive(source: source, destination: root.appendingPathComponent("unsafe.tgz"))
        let client = fixtureClient(archive: archive)
        await #expect(throws: NativeConnectorError.self) {
            _ = try await client.resolveArchive(repositoryURL: "https://github.com/test/program", tag: "v1.0.0", assetName: "program-macos-universal.tgz")
        }
        for url in ["http://github.com/a/b", "https://token@github.com/a/b", "https://github.com/a/b/tree/main", "https://evil.test/a/b"] {
            #expect(throws: NativeConnectorError.self) { _ = try NativePluginRepositoryClient.Repository(url) }
        }
        for path in ["../tool", "/tool", "dir/../../tool", "dir\\tool"] {
            #expect(throws: NativeConnectorError.self) { try NativePluginRepositoryClient.validateRelativePath(path) }
        }
    }

    @Test("repository documents and releases are exposed as metadata without requiring a plugin manifest")
    func discoversProgramRepository() async throws {
        let client = NativePluginRepositoryClient(fetch: { url, _ in
            if url.path.hasSuffix("/readme") || url.path.contains("/contents/") {
                return try JSONSerialization.data(withJSONObject: ["encoding": "base64", "content": Data("Run program mcp".utf8).base64EncodedString()])
            }
            if url.path.contains("/git/trees/") {
                return try JSONSerialization.data(withJSONObject: ["tree": [["path": "docs/mcp.md"], ["path": "apps/cli/README.md"]], "truncated": false])
            }
            return try JSONSerialization.data(withJSONObject: ["tag_name": "v1.0.0", "draft": false, "assets": []])
        })
        let result = try await client.discover(repositoryURL: "https://github.com/test/program.git")
        let object = try #require(result.objectValue)
        #expect(object["readme"] == .string("Run program mcp"))
        #expect(object["documentation_paths"] == .array([.string("docs/mcp.md"), .string("apps/cli/README.md")]))
        let document = try await client.discover(repositoryURL: "https://github.com/test/program", documentPath: "docs/mcp.md", reference: "v1.0.0")
        #expect(document.objectValue?["document"] == .string("Run program mcp"))
    }

    @Test("release resolution rejects substituted origins and mismatched published digests")
    func verifiesReleaseProvenance() async throws {
        let payload = Data("not an archive".utf8)
        for (url, digest) in [
            ("https://evil.test/test/program/releases/download/v1.0.0/tool.zip", NativePluginRepositoryClient.digest(payload)),
            ("https://github.com/test/program/releases/download/v1.0.0/tool.zip", String(repeating: "0", count: 64)),
        ] {
            let client = NativePluginRepositoryClient(fetch: { request, _ in
                if request.host != "api.github.com" { return payload }
                return try JSONSerialization.data(withJSONObject: ["tag_name": "v1.0.0", "draft": false, "assets": [[
                    "name": "tool.zip", "size": payload.count, "browser_download_url": url, "digest": "sha256:" + digest,
                ]]])
            })
            await #expect(throws: NativeConnectorError.self) {
                _ = try await client.downloadRelease(repo: .init("https://github.com/test/program"), tag: "v1.0.0", assetName: "tool.zip")
            }
        }
    }

    @Test("a program that cannot speak MCP is never persisted as installed")
    func rejectsNonMCPExecutable() async throws {
        let root = temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("not-mcp")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data("#!/bin/sh\nexit 1\n".utf8).write(to: executable)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: executable.path)
        let service = try service(root)
        let draft = LocalConnectorPluginSourceDraft(kind: .localExecutable, location: executable.path, exactVersion: "1.0.0",
            program: .init(displayName: "not MCP", componentKey: "not-mcp", executablePath: executable.path,
                           arguments: [], accessProfile: .restricted))
        await #expect(throws: (any Error).self) { _ = try await service.inspectPluginSource(draft) }
        #expect(await service.installerPluginRecords().isEmpty)
    }

    @Test("real Craft Anything official release installs, persists, initializes and executes a read-only MCP tool",
          .enabled(if: ProcessInfo.processInfo.environment["CHATOS_CRAFT_INTEGRATION"] == "1"))
    func craftAnythingAcceptance() async throws {
        let root = temporaryRoot()
        defer { try? FileManager.default.removeItem(at: root) }
        let env = ProcessInfo.processInfo.environment
        let client: NativePluginRepositoryClient
        if let cache = env["CHATOS_CRAFT_ACCEPTANCE_DIR"] {
            let release = try Data(contentsOf: URL(fileURLWithPath: cache).appendingPathComponent("release.json"))
            let archive = try Data(contentsOf: URL(fileURLWithPath: cache).appendingPathComponent("craft.zip"))
            let readme = try Data(contentsOf: URL(fileURLWithPath: cache).appendingPathComponent("readme.json"))
            // Transport-only cache: real release metadata/digest and program bytes stay unmodified.
            client = .init(fetch: { url, _ in
                if url.host == "github.com" { return archive }
                if url.path.hasSuffix("/readme") { return readme }
                if url.path.contains("/git/trees/") { return Data("{\"tree\":[]}".utf8) }
                return release
            })
        } else { client = .init() }
        let repository = "https://github.com/storytold/craft-anything.git"
        let discovery = try #require(try await client.discover(repositoryURL: repository).objectValue)
        let release = try #require(discovery["release"]?.objectValue)
        guard case let .string(tag)? = release["tag"] else { throw URLError(.cannotParseResponse) }
        guard case let .string(readme)? = discovery["readme"] else { throw URLError(.cannotParseResponse) }
        #expect(readme.contains("craft-anything-cli mcp"))
        let version = tag.hasPrefix("v") ? String(tag.dropFirst()) : tag
        let asset = "craft-anything-cli-\(version)-macos-universal.zip"
        let resolved = try await client.resolveArchive(repositoryURL: repository, tag: tag, assetName: asset)
        let entry = try #require(resolved.archiveEntries.first { $0.hasSuffix("/craft-anything-cli") })
        let source = LocalConnectorPluginSourceDraft(kind: .githubRelease, location: repository, exactVersion: tag,
            program: .init(displayName: "Craft Anything", componentKey: "craft-anything", executablePath: entry,
                           arguments: ["mcp"], accessProfile: .developerAnalysis, assetName: asset, archiveSHA256: resolved.archiveSHA256))
        let service = try service(root, client: client)
        var request = try #require(NativePluginInstallerMetadata.source(source).objectValue)
        request["replacing_plugin_id"] = .string("REPLACE")
        await #expect(throws: NativeConnectorError.self) {
            _ = try await service.inspectPluginSource(
                NativeLocalAgentPluginInstallerToolExecutor.pluginSource(request),
                replacingPluginID: NativeLocalAgentPluginInstallerToolExecutor.replacementPluginID(request)
            )
        }
        request["replacing_plugin_id"] = .null
        let parsedSource = try NativeLocalAgentPluginInstallerToolExecutor.pluginSource(request)
        let replacement = try NativeLocalAgentPluginInstallerToolExecutor.replacementPluginID(request)
        let inspection = try await service.inspectPluginSource(parsedSource, replacingPluginID: replacement)
        let names = try #require(inspection.components.first?.toolNames)
        try #require(names.contains("core_targets_list"))
        _ = try await service.installPluginSource(parsedSource, expectedFingerprint: inspection.fingerprint, replacingPluginID: replacement)
        let record = try #require(await service.installerPluginRecords().values.first)
        #expect(record.sourceDraft == source)
        #expect(NativePluginInstallationIntegrity.status(record: record) == .verified)
        let result = try await callInfo(record: record, runtime: root.appendingPathComponent("runtime"), tool: "core_targets_list", arguments: .object([:]))
        try #require(result.jsonObject?["isError"]?.jsonBool != true)
        try #require(result.jsonObject?["content"]?.jsonArray?.isEmpty == false)
        #expect(try await service.installedAgentPlugins(ownerUserID: "program-owner").contains { $0.pluginID == record.pluginID })
        let catalog = await service.customInstalledPlugins(forceRefresh: true)
        let visible = try #require(catalog.first { $0.pluginID == record.pluginID })
        #expect(visible.runtimeStatus == .ready)
        #expect(visible.runtimeToolCount == names.count)
        print("Craft Anything acceptance: \(tag), \(names.count) tools, core_targets_list succeeded, managed install verified")
    }

    private func fixtureClient(archive: Data) -> NativePluginRepositoryClient {
        .init(fetch: { url, _ in
            if url.host == "github.com" { return archive }
            let tag = url.lastPathComponent
            return try JSONSerialization.data(withJSONObject: ["tag_name": tag, "draft": false, "assets": [[
                "name": "program-macos-universal.tgz", "size": archive.count,
                "browser_download_url": "https://github.com/test/program/releases/download/\(tag)/program-macos-universal.tgz",
                "digest": "sha256:" + NativePluginRepositoryClient.digest(archive),
            ]]])
        })
    }

    private func temporaryRoot() -> URL {
        FileManager.default.temporaryDirectory.appendingPathComponent("ProgramMCP-\(UUID().uuidString)")
    }

    private func service(_ root: URL, client: NativePluginRepositoryClient = .init()) throws -> NativeLocalConnectorService {
        let stateURL = root.appendingPathComponent("state.json")
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "program-owner", username: "program", displayName: "Program", role: "user")
        state.deviceID = "program-device"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        return NativeLocalConnectorService(
            configuration: .init(gatewayBaseURL: URL(string: "https://connector.example.test")!, stateURL: stateURL),
            ticketProvider: ProgramRejectingTicketProvider(),
            pluginInstallerFactory: { NativePluginInstaller(rootURL: $0, npmInstall: { _ in }, repositoryClient: client) }
        )
    }

    private func writeExecutable(_ url: URL) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(#"""
        #!/usr/bin/env node
        if (JSON.stringify(process.argv.slice(2)) !== JSON.stringify(["mcp", "a b", "$(touch nope)"])) process.exit(9);
        require("node:readline").createInterface({input:process.stdin}).on("line", line => {
          const q = JSON.parse(line); if (q.id === undefined) return;
          let result = {};
          if (q.method === "initialize") result = {protocolVersion:"2024-11-05",capabilities:{tools:{}},serverInfo:{name:"program-fixture",version:"1.0.0"}};
          if (q.method === "tools/list") result = {tools:[{name:"fixture_info",description:"Read fixture info",inputSchema:{type:"object",properties:{}}}]};
          if (q.method === "tools/call") result = {content:[{type:"text",text:"fixture ok"}],isError:false};
          process.stdout.write(JSON.stringify({jsonrpc:"2.0",id:q.id,result})+"\n");
        });
        """#.utf8).write(to: url)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
    }

    private func makeArchive(source: URL, destination: URL) async throws -> Data {
        _ = try await NativePluginInstaller.runNPMProcess(executableURL: URL(fileURLWithPath: "/usr/bin/tar"),
            arguments: ["-czf", destination.path, "-C", source.path, "."], currentDirectoryURL: nil, timeout: 30,
            stderrLimit: 65_536, overflowMessage: "fixture overflow", timeoutMessage: "fixture timeout", failureLabel: "fixture archive")
        return try Data(contentsOf: destination)
    }

    private func callInfo(record: NativeInstalledPluginRecord, runtime: URL, tool: String = "fixture_info", arguments: NativeJSONValue = .object([:])) async throws -> NativeJSONValue {
        let manifest = try NativePluginManifestLoader.loadManifest(from: URL(fileURLWithPath: record.installationPath).appendingPathComponent("chatos.plugin.json"))
        let launch = try NativePluginManifestLoader.prepare(record: record, manifest: manifest,
            componentKey: try #require(manifest.mcpServers.keys.first), serverKey: nil,
            adapterSessionID: UUID().uuidString, ownerUserID: "program-owner", deviceID: "program-device", workspaceRoot: nil,
            permissionSnapshot: Set(manifest.permissions.map(\.permission)), runtimeRootURL: runtime)
        let client = NativePluginStdioClient(launch: launch)
        do {
            try await client.start()
            _ = try await client.initialize()
            let result = try await client.callTool(name: tool, arguments: arguments, timeout: .seconds(30))
            await client.terminate()
            return result
        } catch { await client.terminate(); throw error }
    }
}

private struct ProgramRejectingTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { throw URLError(.notConnectedToInternet) }
}

private extension LocalAgentJSONValue {
    var objectValue: [String: LocalAgentJSONValue]? {
        guard case let .object(value) = self else { return nil }
        return value
    }
}
