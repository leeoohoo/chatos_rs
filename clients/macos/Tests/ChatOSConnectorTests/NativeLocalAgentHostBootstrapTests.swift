// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

@testable import ChatOSConnector
import Foundation
import Testing

@Suite("Native local Agent Host bootstrap", .serialized)
struct NativeLocalAgentHostBootstrapTests {
    @Test("builds the exact SQLite launch contract from Keychain references")
    func buildsSQLiteLaunch() async throws {
        let fixture = try await BootstrapFixture()
        defer { fixture.cleanup() }
        let database = fixture.root.appendingPathComponent("client.sqlite")

        let configuration = try await fixture.builder.makeConfiguration(settings: fixture.settings(
            storage: .sqlite(
                databaseURL: database,
                encryptionSecretReference: "sqlite-key"
            )
        ))

        let request = try #require(
            JSONSerialization.jsonObject(with: configuration.launchMaterial.snapshotForTesting()) as? [String: Any]
        )
        #expect(request["owner_user_id"] as? String == "user-1")
        #expect(request["attachment_grant_directory"] as? String == fixture.grants.path)
        #expect(request["platform_state_directory"] as? String == fixture.state.path)
        let profile = try #require(request["storage_profile"] as? [String: Any])
        #expect(profile["backend"] as? String == "sqlite")
        #expect(profile["database_path"] as? String == database.path)
        let references = try #require(request["credential_references"] as? [String: Any])
        #expect(references["model_access_token_reference"] as? String == "model-access-token")
        #expect(references["provider_context_key_reference"] as? String == "provider-context-key")
        #expect(request["credentials"] == nil)
        let launchText = String(
            data: try configuration.launchMaterial.snapshotForTesting(),
            encoding: .utf8
        )
        #expect(launchText?.contains("model-token") == false)
        #expect(launchText?.contains(Data(repeating: 7, count: 32).base64EncodedString()) == false)
        #expect(configuration.expectedClientEndpoint.hasSuffix(".sock"))
    }

    @Test("passes only the PostgreSQL secure-store reference")
    func buildsPostgresLaunch() async throws {
        let fixture = try await BootstrapFixture()
        defer { fixture.cleanup() }
        let postgres = NativeLocalAgentPostgresCredential(
            host: "database.example.com",
            database: "chatos",
            username: "chatos-user",
            password: "private-password"
        )
        let configuration = try await fixture.builder.makeConfiguration(settings: fixture.settings(
            storage: .postgres(connectionSecretReference: "postgres-1")
        ))

        let request = try #require(
            JSONSerialization.jsonObject(with: configuration.launchMaterial.snapshotForTesting()) as? [String: Any]
        )
        let profile = try #require(request["storage_profile"] as? [String: Any])
        #expect(profile["connection_secret"] as? String == "postgres-1")
        #expect(request["credentials"] == nil)
        #expect(
            String(data: try configuration.launchMaterial.snapshotForTesting(), encoding: .utf8)?
                .contains("private-password") == false
        )
        #expect(!postgres.debugDescription.contains("private-password"))
        #expect(!postgres.debugDescription.contains("database.example.com"))
    }
}

private struct BootstrapFixture: Sendable {
    let root: URL
    let runtime: URL
    let grants: URL
    let state: URL
    let builder: NativeLocalAgentHostBootstrapBuilder

    init() async throws {
        root = URL(
            fileURLWithPath: "/tmp/chatos-bootstrap-\(UUID().uuidString.prefix(8))",
            isDirectory: true
        )
        runtime = root.appendingPathComponent("runtime", isDirectory: true)
        grants = root.appendingPathComponent("grants", isDirectory: true)
        state = root.appendingPathComponent("state", isDirectory: true)
        builder = NativeLocalAgentHostBootstrapBuilder()
    }

    func settings(storage: NativeLocalAgentStorageBootstrap) -> NativeLocalAgentHostBootstrapSettings {
        NativeLocalAgentHostBootstrapSettings(
            executableURL: URL(fileURLWithPath: "/bin/sh"),
            accountID: "user-1",
            deviceID: "device-1",
            runtimeDirectory: runtime,
            attachmentGrantDirectory: grants,
            platformStateDirectory: state,
            modelGatewayBaseURL: URL(string: "https://api.example.com")!,
            memoryEngineBaseURL: URL(string: "https://memory.example.com")!,
            storage: storage
        )
    }

    func cleanup() {
        try? FileManager.default.removeItem(at: root)
    }
}
