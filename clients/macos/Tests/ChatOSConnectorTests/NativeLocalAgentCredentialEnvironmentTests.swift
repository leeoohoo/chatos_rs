@testable import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Local Agent credential catalog reconciliation")
struct NativeLocalAgentCredentialEnvironmentTests {
    @Test
    func filtersOwnersReferencesMissingAndBlankCredentials() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeLocalAgentModelCredentialStore(rootURL: root)
        try store.saveWithoutUserInteraction("value", ownerUserID: "user-1", modelConfigRef: "active")
        try store.saveWithoutUserInteraction("other", ownerUserID: "user-2", modelConfigRef: "foreign")
        try store.saveWithoutUserInteraction(" \n ", ownerUserID: "user-1", modelConfigRef: "blank")
        try store.saveWithoutUserInteraction("invalid-ref", ownerUserID: "user-1", modelConfigRef: "invalid")
        let snapshots = [
            snapshot("active"), snapshot("foreign", owner: "user-2"),
            snapshot("missing"), snapshot("blank"),
            snapshot("invalid", credentialRef: "env:UNTRUSTED_VARIABLE"),
        ]
        let environment = try NativeLocalAgentCredentialEnvironment.restoredModels(
            snapshots, ownerUserID: "user-1", store: store
        )
        #expect(environment == [store.environmentVariable(modelConfigRef: "active"): "value"])
    }

    @Test
    func currentCatalogRetainsHistoricalKeysAndOverridesOnlyMatchingCredentials() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeLocalAgentModelCredentialStore(rootURL: root)
        try store.saveWithoutUserInteraction("active-value", ownerUserID: "user-1", modelConfigRef: "active")
        try store.saveWithoutUserInteraction("historical-value", ownerUserID: "user-1", modelConfigRef: "historical")
        let snapshots = [snapshot("active"), snapshot("historical")]
        let restored = try NativeLocalAgentCredentialEnvironment.restoredModels(snapshots, ownerUserID: "user-1", store: store)
        var refreshed = try NativeLocalAgentCredentialEnvironment.restoredModels(snapshots, ownerUserID: "user-1", store: store)
        let activeVariable = store.environmentVariable(modelConfigRef: "active")
        // The current selectable catalog contains only active, not historical.
        refreshed[activeVariable] = "active-value"
        #expect(refreshed == restored)
        refreshed[activeVariable] = "rotated-value"
        #expect(refreshed != restored)
        #expect(refreshed[store.environmentVariable(modelConfigRef: "historical")] == "historical-value")
    }

    @Test
    func normalizedModelVariableCollisionsFailClosed() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let store = NativeLocalAgentModelCredentialStore(rootURL: root)
        for model in ["a-b", "a_b"] {
            try store.saveWithoutUserInteraction("test", ownerUserID: "user-1", modelConfigRef: model)
        }
        #expect(throws: NativeLocalAgentBootstrapError.self) {
            try NativeLocalAgentCredentialEnvironment.restoredModels(
                [snapshot("a-b"), snapshot("a_b")], ownerUserID: "user-1", store: store
            )
        }
    }

    private func snapshot(_ model: String, owner: String = "user-1", credentialRef: String? = nil) -> LocalAgentModelConfigSnapshot {
        let variable = NativeLocalAgentModelCredentialStore().environmentVariable(modelConfigRef: model)
        return .init(ownerUserID: owner, modelConfigRef: model, modelConfigRevision: "revision-1",
                     credentialRef: credentialRef ?? "env:\(variable)", baseURL: "https://example.invalid/v1",
                     model: model, provider: "test", supportsResponses: true)
    }
}
