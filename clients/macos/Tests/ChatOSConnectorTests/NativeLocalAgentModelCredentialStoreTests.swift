@testable import ChatOSConnector
import LocalAuthentication
import Security
import Testing

struct NativeLocalAgentModelCredentialStoreTests {
    @Test
    func startupRestoreDisablesKeychainAuthenticationUI() {
        let query = NativeLocalAgentModelCredentialStore.loadQuery(
            ownerUserID: "user-1",
            modelConfigRef: "model-1",
            allowUserInteraction: false
        )

        let context = query[kSecUseAuthenticationContext as String] as? LAContext
        #expect(context?.interactionNotAllowed == true)
        #expect(query[kSecReturnData as String] as? Bool == true)
        #expect(
            query[kSecAttrService as String] as? String
                == "com.chatos.swift.local-agent-model.v2"
        )
        #expect(query[kSecAttrAccount as String] as? String == "v2:user-1:model-1")
    }

    @Test
    func interactiveQueryDoesNotForceAuthenticationUIBehavior() {
        let query = NativeLocalAgentModelCredentialStore.loadQuery(
            ownerUserID: "user-1",
            modelConfigRef: "model-1",
            allowUserInteraction: true
        )

        #expect(query[kSecUseAuthenticationContext as String] == nil)
    }
}
