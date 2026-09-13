// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

@testable import ChatOSConnector
import Foundation
import Security
import Testing

@Suite("Native local Agent Keychain", .serialized)
struct NativeLocalAgentCredentialStoreTests {
    @Test("stores, updates and deletes account-scoped credentials only in Keychain")
    func roundTripsAccountScopedSecrets() async throws {
        let service = "com.chatos.tests.local-agent.\(UUID().uuidString)"
        let store = try NativeLocalAgentCredentialStore(
            service: service,
            broker: testKeychainBroker()
        )
        guard await store.isAvailableForNonInteractiveAccess() else { return }
        let reference = "sqlite-device-key"
        func cleanUp() async {
            try? await store.delete(accountID: "user-1", reference: reference)
            try? await store.delete(accountID: "user-2", reference: reference)
        }

        do {
            try await store.save(
                Data("first-secret".utf8),
                accountID: "user-1",
                reference: reference
            )
            try await store.save(
                Data("second-secret".utf8),
                accountID: "user-2",
                reference: reference
            )
            try await store.save(
                Data("updated-secret".utf8),
                accountID: "user-1",
                reference: reference
            )

            #expect(
                try await store.load(accountID: "user-1", reference: reference)
                    == Data("updated-secret".utf8)
            )
            #expect(
                try await store.load(accountID: "user-2", reference: reference)
                    == Data("second-secret".utf8)
            )
            try await store.delete(accountID: "user-1", reference: reference)
            #expect(try await store.load(accountID: "user-1", reference: reference) == nil)
            #expect(try await store.load(accountID: "user-2", reference: reference) != nil)
        } catch {
            await cleanUp()
            throw error
        }
        await cleanUp()
    }

    @Test("rejects empty, padded and control-character references")
    func rejectsUnsafeReferences() async throws {
        let store = try NativeLocalAgentCredentialStore(
            service: "com.chatos.tests.local-agent.\(UUID().uuidString)",
            broker: testKeychainBroker()
        )
        for reference in ["", " padded", "line\nbreak"] {
            await #expect(throws: NativeLocalAgentCredentialStoreError.invalidReference) {
                _ = try await store.load(accountID: "user-1", reference: reference)
            }
        }
    }

    @Test("fails without requesting UI when the login Keychain is locked")
    func rejectsLockedKeychainWithoutInteraction() async throws {
        let store = try NativeLocalAgentCredentialStore(
            service: "com.chatos.tests.local-agent.\(UUID().uuidString)",
            broker: testKeychainBroker()
        )
        guard !(await store.isAvailableForNonInteractiveAccess()) else { return }
        await #expect(
            throws: NativeLocalAgentCredentialStoreError.keychain(
                operation: "load",
                status: errSecInteractionNotAllowed
            )
        ) {
            _ = try await store.load(accountID: "user-1", reference: "access-token")
        }
    }

    @Test("production services reject callers outside the bundled ChatOS app")
    func productionServicesRejectTestParent() throws {
        let broker = testKeychainBroker()

        #expect(throws: MacOSKeychainBrokerError.status(errSecParam)) {
            _ = try broker.load(
                service: "com.chatos.swift-client.authentication.v5",
                account: "access-token"
            )
        }
    }

    @Test("test services reject brokers copied outside the package build directory")
    func testServicesCannotEscapeBuildBroker() throws {
        let temporaryDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-keychain-broker-security-\(UUID().uuidString)")
        try FileManager.default.createDirectory(
            at: temporaryDirectory,
            withIntermediateDirectories: true
        )
        defer { try? FileManager.default.removeItem(at: temporaryDirectory) }

        let copiedBroker = temporaryDirectory
            .appendingPathComponent("ChatOSKeychainBroker", isDirectory: false)
        try FileManager.default.copyItem(
            at: testKeychainBrokerURL(),
            to: copiedBroker
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: copiedBroker.path
        )
        let broker = MacOSKeychainBrokerClient(executableURL: copiedBroker)

        #expect(throws: MacOSKeychainBrokerError.status(errSecParam)) {
            _ = try broker.load(
                service: "com.chatos.tests.isolation.\(UUID().uuidString)",
                account: "access-token"
            )
        }
    }

    private func testKeychainBroker() -> MacOSKeychainBrokerClient {
        MacOSKeychainBrokerClient(executableURL: testKeychainBrokerURL())
    }

    private func testKeychainBrokerURL() -> URL {
        let packageDirectory = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        return packageDirectory
            .appendingPathComponent(".build", isDirectory: true)
            .appendingPathComponent("arm64-apple-macosx", isDirectory: true)
            .appendingPathComponent("debug", isDirectory: true)
            .appendingPathComponent("ChatOSKeychainBroker", isDirectory: false)
    }
}
