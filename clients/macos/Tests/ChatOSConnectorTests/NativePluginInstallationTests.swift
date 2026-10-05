@testable import ChatOSConnector
import ChatOSCore
import Darwin
import Foundation
import Testing

extension NativePluginRuntimeTests {
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
        let ui = root.appendingPathComponent("ui", isDirectory: true)
        try FileManager.default.createDirectory(at: ui, withIntermediateDirectories: true)
        try Data("<html><body>Fixture</body></html>".utf8)
            .write(to: ui.appendingPathComponent("index.html"))
        try Data("""
        {
          "schemaVersion": 3,
          "name": "fixture",
          "version": "1.0.0",
          "skills": ["./skills/fixture-skill"],
          "mcpServers": {
            "fixture-mcp": {"type": "stdio", "bin": "fixture", "args": []}
          },
          "ui": [{
            "componentKey": "fixture-workbench",
            "source": "./ui/index.html",
            "surface": "workbench",
            "runtime": {"type": "local_http", "bin": "fixture", "args": ["studio"]}
          }],
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
            GatewayPluginComponentStatus(
                componentKey: "fixture-workbench",
                kind: "ui_contribution",
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
        #expect(statuses[2]["kind"] as? String == "ui_contribution")
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

    @Test("plugin Skill v2 separates catalog, activation and resource reads")
    func pluginSkillV2ProgressiveLoading() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let skill = root.appendingPathComponent("skills/fixture-skill", isDirectory: true)
        let references = skill.appendingPathComponent("references", isDirectory: true)
        try FileManager.default.createDirectory(at: references, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let instructions = Data("""
        ---
        name: fixture-skill
        description: Fixture Skill instructions.
        ---
        # Fixture Skill
        Read the guide only when needed.
        """.utf8)
        let guide = Data("# Guide\nUse fresh screenshots.\n".utf8)
        try instructions.write(to: skill.appendingPathComponent("SKILL.md"))
        try guide.write(to: references.appendingPathComponent("guide.md"))
        try Data("""
        {
          "schemaVersion": 3,
          "name": "fixture",
          "version": "1.0.0",
          "skills": ["./skills/fixture-skill"],
          "mcpServers": {}
        }
        """.utf8).write(to: root.appendingPathComponent("chatos.plugin.json"))
        let resource: NativeJSONValue = .object([
            "relative_path": .string("references/guide.md"),
            "kind": .string("reference"),
            "size_bytes": .number(Double(guide.count)),
            "sha256": .string(NativePluginHash.sha256(guide)),
        ])
        let resourceManifestSHA256 = try NativePluginHash.canonicalSHA256(.array([resource]))
        let metadata: NativeJSONValue = .object([
            "name": .string("fixture-skill"),
            "description": .string("Fixture Skill instructions."),
            "role": .string("leaf"),
            "activation_policy": .string("model_or_user"),
            "context_mode": .string("inline"),
            "required_skills": .array([]),
            "related_skills": .array([]),
            "extra": .object([:]),
        ])
        let instructionsSHA256 = NativePluginHash.sha256(instructions)
        let snapshotSHA256 = try NativePluginHash.canonicalSHA256(.object([
            "protocol_version": .number(2),
            "skill_id": .string("fixture-skill"),
            "relative_skill_path": .string("skills/fixture-skill/SKILL.md"),
            "metadata": metadata,
            "instructions_sha256": .string(instructionsSHA256),
            "resource_manifest_sha256": .string(resourceManifestSHA256),
        ]))
        let expectedSnapshot: NativeJSONValue = .object([
            "protocol_version": .number(2),
            "skill_id": .string("fixture-skill"),
            "relative_skill_path": .string("skills/fixture-skill/SKILL.md"),
            "metadata": metadata,
            "instructions_sha256": .string(instructionsSHA256),
            "resource_manifest_sha256": .string(resourceManifestSHA256),
            "resources": .array([resource]),
            "snapshot_sha256": .string(snapshotSHA256),
        ])
        let record = NativeInstalledPluginRecord(
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "1.0.0",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: root.path,
            installedAt: "2026-09-04T00:00:00Z"
        )

        let prepared = try NativePluginSkillSnapshotLoader.prepareV2Body(
            record: record,
            componentKey: "fixture-skill",
            expectedSnapshot: expectedSnapshot,
            runID: "run-1",
            adapterSessionID: "adapter-1",
            now: Date(timeIntervalSince1970: 0)
        )
        let preparedObject = try prepared.requireObject()
        #expect(try preparedObject.requireStringArray("operations") == [
            "skill_activate", "skill_read_resource",
        ])
        let catalog = try #require(preparedObject["skills"]?.jsonArray?.first?.jsonObject)
        #expect(catalog["instructions"] == nil)

        let activated = try NativePluginSkillSnapshotLoader.activateV2(
            record: record,
            componentKey: "fixture-skill",
            expectedSnapshot: expectedSnapshot
        )
        #expect(try activated.requireObject().requireString("instructions").contains("# Fixture Skill"))

        let resourceRead = try NativePluginSkillSnapshotLoader.readV2Resource(
            record: record,
            componentKey: "fixture-skill",
            expectedSnapshot: expectedSnapshot,
            relativePath: "references/guide.md",
            offset: 0,
            maximumCharacters: 8
        )
        let resourceObject = try resourceRead.requireObject()
        let expectedPage = String(String(decoding: guide, as: UTF8.self).prefix(8))
        let actualPage = try #require(resourceObject["content"]?.jsonString)
        #expect(actualPage == expectedPage)
        #expect(resourceObject["truncated"]?.jsonBool == true)
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
                installedAt: "2026-08-26T00:00:00Z",
                packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
                    installationURL: installation,
                    maximumFiles: 100,
                    maximumBytes: 1_024 * 1_024
                )
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
        let otherUserLaunch = try NativePluginManifestLoader.prepare(
            record: .init(
                pluginID: "plugin-1",
                releaseID: "release-1",
                version: "0.3.42",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: installation.path,
                installedAt: "2026-08-26T00:00:00Z",
                packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
                    installationURL: installation,
                    maximumFiles: 100,
                    maximumBytes: 1_024 * 1_024
                )
            ),
            componentKey: "computer-use",
            serverKey: nil,
            adapterSessionID: "adapter-1",
            ownerUserID: "user-2",
            deviceID: "device-1",
            workspaceRoot: root,
            permissionSnapshot: ["process.spawn"],
            runtimeRootURL: root.appendingPathComponent("runtime", isDirectory: true)
        )

        #expect(launch.executableURL == launcher.standardizedFileURL)
        #expect(launch.arguments == ["mcp"])
        #expect(launch.environment["CHATOS_PLUGIN_RUNTIME_SESSION_ID"] == "adapter-1")
        #expect(launch.environment["CHATOS_WORKSPACE"] == nil)
        for key in [
            "CHATOS_PLUGIN_VISUAL_SESSION_DIR",
            "CHATOS_PLUGIN_ARTIFACT_DIR",
            "CHATOS_PLUGIN_FILE_GRANT_DIR",
        ] {
            #expect(launch.environment[key] != otherUserLaunch.environment[key])
            #expect(launch.environment[key]?.contains("/users/") == true)
        }
#if os(macOS)
        let managedAppRoot = launch.environment["VISUAL_COMPUTER_USE_MANAGED_APP_ROOT"]
        #expect(managedAppRoot?.contains("/data/users/") == true)
        #expect(managedAppRoot?.hasSuffix("/managed-app") == true)
        #expect(launch.environment["OPEN_COMPUTER_USE_MANAGED_APP_ROOT"] == managedAppRoot)
        #expect(
            managedAppRoot
                != otherUserLaunch.environment["VISUAL_COMPUTER_USE_MANAGED_APP_ROOT"]
        )
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
                installedAt: "2026-08-29T00:00:00Z",
                packageFileSHA256: try NativePluginInstallationIntegrity.snapshot(
                    installationURL: installation,
                    maximumFiles: 100,
                    maximumBytes: 1_024 * 1_024
                )
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

    @Test("preloaded manifest launch still fails closed when an installed file disappears")
    func preloadedManifestLaunchRevalidatesInstallationIntegrity() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let installation = root.appendingPathComponent("plugin", isDirectory: true)
        let launcher = installation
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent("fixture")
        let manifestURL = installation.appendingPathComponent("chatos.plugin.json")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(
            at: launcher.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("#!/bin/sh\n".utf8).write(to: launcher)
        try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: launcher.path)
        try Data("""
        {"schemaVersion":3,"name":"fixture","version":"1.0.0","mcpServers":{"fixture":{"type":"stdio","bin":"fixture"}},"permissions":[{"permission":"process.spawn","required":true,"components":["fixture"]}]}
        """.utf8).write(to: manifestURL)

        let manifest = try NativePluginManifestLoader.loadManifest(from: manifestURL)
        let checksums = try NativePluginInstallationIntegrity.snapshot(
            installationURL: installation,
            maximumFiles: 100,
            maximumBytes: 1_024 * 1_024
        )
        try FileManager.default.removeItem(at: manifestURL)
        #expect(throws: NativePluginRuntimeError.self) {
            _ = try NativePluginManifestLoader.prepare(
                record: .init(
                pluginID: "plugin-1",
                releaseID: "release-1",
                version: "1.0.0",
                artifactSHA256: String(repeating: "a", count: 64),
                installationPath: installation.path,
                installedAt: "2026-10-03T00:00:00Z",
                packageFileSHA256: checksums
                ),
                manifest: manifest,
                componentKey: "fixture",
                serverKey: nil,
                adapterSessionID: "adapter-preloaded",
                ownerUserID: "user-1",
                deviceID: "device-1",
                workspaceRoot: nil,
                permissionSnapshot: ["process.spawn"],
                runtimeRootURL: root.appendingPathComponent("runtime", isDirectory: true)
            )
        }
    }

}
