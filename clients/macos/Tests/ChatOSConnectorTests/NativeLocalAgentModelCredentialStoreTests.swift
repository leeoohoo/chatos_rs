@testable import ChatOSConnector
import Foundation
import Testing

struct NativeLocalAgentModelCredentialStoreTests {
    @Test("explicit production path still uses the production vault")
    func recognizesExplicitProductionPath() {
        #expect(NativeLocalAgentModelCredentialStore.usesProductionVault(
            rootURL: NativeLocalAgentModelCredentialStore.productionRootURL
        ))
        #expect(!NativeLocalAgentModelCredentialStore.usesProductionVault(
            rootURL: FileManager.default.temporaryDirectory
        ))
    }

    @Test
    func credentialsRoundTripThroughKeychain() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeLocalAgentModelCredentialStore(rootURL: root)
        defer { try? store.delete(ownerUserID: "user-1", modelConfigRef: "model-1") }

        #expect(try store.loadWithoutUserInteraction(
            ownerUserID: "user-1",
            modelConfigRef: "model-1"
        ) == nil)
        #expect(try store.saveWithoutUserInteraction(
            "secret-value",
            ownerUserID: "user-1",
            modelConfigRef: "model-1"
        ))
        #expect(try store.loadWithoutUserInteraction(
            ownerUserID: "user-1",
            modelConfigRef: "model-1"
        ) == "secret-value")

        try store.delete(ownerUserID: "user-1", modelConfigRef: "model-1")
        #expect(try store.loadWithoutUserInteraction(
            ownerUserID: "user-1",
            modelConfigRef: "model-1"
        ) == nil)
    }

    @Test
    func legacyCredentialFilesMigrateToKeychainAndAreDeleted() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeLocalAgentModelCredentialStore(rootURL: root)
        defer { try? store.delete(ownerUserID: "sensitive-user", modelConfigRef: "sensitive-model") }
        let url = store.credentialURL(
            ownerUserID: "sensitive-user",
            modelConfigRef: "sensitive-model"
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data("secret-value".utf8).write(to: url)

        #expect(url.lastPathComponent.hasPrefix("model-"))
        #expect(!url.lastPathComponent.contains("sensitive-user"))
        #expect(!url.lastPathComponent.contains("sensitive-model"))
        #expect(try store.loadWithoutUserInteraction(
            ownerUserID: "sensitive-user",
            modelConfigRef: "sensitive-model"
        ) == "secret-value")
        #expect(!FileManager.default.fileExists(atPath: url.path))
    }

    @Test
    func credentialsAreIsolatedByOwnerAndModel() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeLocalAgentModelCredentialStore(rootURL: root)
        defer { try? store.delete(ownerUserID: "user-1", modelConfigRef: "model-1") }

        try store.saveWithoutUserInteraction(
            "first",
            ownerUserID: "user-1",
            modelConfigRef: "model-1"
        )

        #expect(try store.loadWithoutUserInteraction(
            ownerUserID: "user-2",
            modelConfigRef: "model-1"
        ) == nil)
        #expect(try store.loadWithoutUserInteraction(
            ownerUserID: "user-1",
            modelConfigRef: "model-2"
        ) == nil)
    }
}
