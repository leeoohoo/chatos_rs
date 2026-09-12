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
        let profile = try #require(request["storage_profile"] as? [String: Any])
        #expect(profile["backend"] as? String == "sqlite")
        #expect(profile["database_path"] as? String == database.path)
        let credentials = try #require(request["credentials"] as? [String: Any])
        #expect(credentials["model_access_token"] as? String == "model-token")
        #expect(credentials["provider_context_key_base64"] as? String == Data(repeating: 3, count: 32).base64EncodedString())
        let storage = try #require(credentials["storage"] as? [String: Any])
        #expect(storage["backend"] as? String == "sqlite")
        #expect(storage["encryption_key_base64"] as? String == Data(repeating: 7, count: 32).base64EncodedString())
        #expect(configuration.expectedClientEndpoint.hasSuffix(".sock"))
    }

    @Test("builds verified TLS PostgreSQL credentials without logging them")
    func buildsPostgresLaunch() async throws {
        let fixture = try await BootstrapFixture()
        defer { fixture.cleanup() }
        let postgres = NativeLocalAgentPostgresCredential(
            host: "database.example.com",
            database: "chatos",
            username: "chatos-user",
            password: "private-password"
        )
        try await fixture.store.save(
            try JSONEncoder().encode(postgres),
            accountID: "user-1",
            reference: "postgres-1"
        )

        let configuration = try await fixture.builder.makeConfiguration(settings: fixture.settings(
            storage: .postgres(connectionSecretReference: "postgres-1")
        ))

        let request = try #require(
            JSONSerialization.jsonObject(with: configuration.launchMaterial.snapshotForTesting()) as? [String: Any]
        )
        let credentials = try #require(request["credentials"] as? [String: Any])
        let storage = try #require(credentials["storage"] as? [String: Any])
        #expect(storage["tls_mode"] as? String == "verify_full")
        #expect(storage["password"] as? String == "private-password")
        #expect(!postgres.debugDescription.contains("private-password"))
        #expect(!postgres.debugDescription.contains("database.example.com"))
    }
}

private struct BootstrapFixture: Sendable {
    let root: URL
    let runtime: URL
    let grants: URL
    let service: String
    let store: NativeLocalAgentCredentialStore
    let builder: NativeLocalAgentHostBootstrapBuilder

    init() async throws {
        root = URL(
            fileURLWithPath: "/tmp/chatos-bootstrap-\(UUID().uuidString.prefix(8))",
            isDirectory: true
        )
        runtime = root.appendingPathComponent("runtime", isDirectory: true)
        grants = root.appendingPathComponent("grants", isDirectory: true)
        service = "com.chatos.tests.local-agent.bootstrap.\(UUID().uuidString)"
        store = try NativeLocalAgentCredentialStore(service: service)
        builder = NativeLocalAgentHostBootstrapBuilder(credentials: store)
        try await store.save(
            Data("model-token".utf8),
            accountID: "user-1",
            reference: NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference
        )
        try await store.save(
            Data(repeating: 3, count: 32),
            accountID: "user-1",
            reference: NativeLocalAgentHostBootstrapBuilder.providerContextKeyReference
        )
        try await store.save(
            Data(repeating: 7, count: 32),
            accountID: "user-1",
            reference: "sqlite-key"
        )
    }

    func settings(storage: NativeLocalAgentStorageBootstrap) -> NativeLocalAgentHostBootstrapSettings {
        NativeLocalAgentHostBootstrapSettings(
            executableURL: URL(fileURLWithPath: "/bin/sh"),
            accountID: "user-1",
            deviceID: "device-1",
            runtimeDirectory: runtime,
            attachmentGrantDirectory: grants,
            modelGatewayBaseURL: URL(string: "https://api.example.com")!,
            memoryEngineBaseURL: URL(string: "https://memory.example.com")!,
            storage: storage
        )
    }

    func cleanup() {
        let store = store
        Task {
            for reference in [
                NativeLocalAgentHostBootstrapBuilder.modelAccessTokenReference,
                NativeLocalAgentHostBootstrapBuilder.providerContextKeyReference,
                "sqlite-key",
                "postgres-1",
            ] {
                try? await store.delete(accountID: "user-1", reference: reference)
            }
        }
        try? FileManager.default.removeItem(at: root)
    }
}
