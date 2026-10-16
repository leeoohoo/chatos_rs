import ChatOSCore
import Foundation

enum NativeLocalAgentCredentialEnvironment {
    /// Historical Runs freeze model references. Keep their existing child-process
    /// credentials when the selectable catalog changes; this does not add model choices
    /// or grant capabilities. New catalog credentials override the matching model key.
    static func restoredModels(
        _ snapshots: [LocalAgentModelConfigSnapshot],
        ownerUserID: String,
        store: NativeLocalAgentModelCredentialStore
    ) throws -> [String: String] {
        var environment: [String: String] = [:]
        for snapshot in snapshots where snapshot.ownerUserID == ownerUserID {
            let variable = store.environmentVariable(modelConfigRef: snapshot.modelConfigRef)
            guard snapshot.credentialRef == "env:\(variable)",
                  let credential = try store.loadWithoutUserInteraction(
                    ownerUserID: ownerUserID, modelConfigRef: snapshot.modelConfigRef
                  )?.trimmingCharacters(in: .whitespacesAndNewlines),
                  !credential.isEmpty else { continue }
            guard environment[variable] == nil else {
                throw NativeLocalAgentBootstrapError.credentialVariableCollision
            }
            environment[variable] = credential
        }
        return environment
    }
}
