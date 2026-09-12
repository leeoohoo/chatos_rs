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
        let store = try NativeLocalAgentCredentialStore(service: service)
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
            service: "com.chatos.tests.local-agent.\(UUID().uuidString)"
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
            service: "com.chatos.tests.local-agent.\(UUID().uuidString)"
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
}
