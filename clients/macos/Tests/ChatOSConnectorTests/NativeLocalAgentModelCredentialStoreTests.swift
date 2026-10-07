@testable import ChatOSConnector
import Foundation
import Testing

struct NativeLocalAgentModelCredentialStoreTests {
    @Test
    func credentialsRoundTripWithoutUsingKeychain() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeLocalAgentModelCredentialStore(rootURL: root)

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
    func credentialFilesArePrivateAndDoNotExposeAccountIdentifiers() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeLocalAgentModelCredentialStore(rootURL: root)

        try store.saveWithoutUserInteraction(
            "secret-value",
            ownerUserID: "sensitive-user",
            modelConfigRef: "sensitive-model"
        )
        let url = store.credentialURL(
            ownerUserID: "sensitive-user",
            modelConfigRef: "sensitive-model"
        )
        let rootAttributes = try FileManager.default.attributesOfItem(atPath: root.path)
        let fileAttributes = try FileManager.default.attributesOfItem(atPath: url.path)

        #expect(url.lastPathComponent.hasPrefix("model-"))
        #expect(!url.lastPathComponent.contains("sensitive-user"))
        #expect(!url.lastPathComponent.contains("sensitive-model"))
        #expect((rootAttributes[.posixPermissions] as? NSNumber)?.intValue == 0o700)
        #expect((fileAttributes[.posixPermissions] as? NSNumber)?.intValue == 0o600)
    }

    @Test
    func credentialsAreIsolatedByOwnerAndModel() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeLocalAgentModelCredentialStore(rootURL: root)

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
