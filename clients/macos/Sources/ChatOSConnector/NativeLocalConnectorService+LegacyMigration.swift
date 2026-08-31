import ChatOSCore
import Foundation

extension NativeLocalConnectorService {
    func importLegacyWorkspacesIfNeeded() async throws {
        guard let deviceID = state.deviceID else { return }
        let legacyURL = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".chatos", isDirectory: true)
            .appendingPathComponent("local_connector", isDirectory: true)
            .appendingPathComponent("state.json", isDirectory: false)
        guard let data = try? Data(contentsOf: legacyURL),
              let legacy = try? JSONDecoder().decode(LegacyConnectorState.self, from: data) else { return }

        let candidates = legacy.workspaces.compactMap { workspace -> LegacyWorkspace? in
            guard !workspace.fingerprint.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                  !workspace.absoluteRoot.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
            var isDirectory: ObjCBool = false
            guard FileManager.default.fileExists(atPath: workspace.absoluteRoot, isDirectory: &isDirectory), isDirectory.boolValue else { return nil }
            return workspace
        }
        let existingFingerprints = Set(state.workspaces.map(\.fingerprint))
        let missing = candidates.filter { !existingFingerprints.contains($0.fingerprint) }
        guard !missing.isEmpty else { return }

        let token = try requireAccessToken()
        let remoteWorkspaces = try await gateway.listWorkspaces(token: token)
        var imported = state.workspaces
        for workspace in missing {
            let remote: GatewayWorkspaceDTO
            if let existing = remoteWorkspaces.first(where: {
                $0.deviceID == deviceID && $0.localPathFingerprint == workspace.fingerprint
            }) {
                remote = existing
            } else {
                remote = try await gateway.createWorkspace(
                    token: token,
                    deviceID: deviceID,
                    alias: workspace.alias,
                    fingerprint: workspace.fingerprint
                )
            }
            imported.append(
                LocalConnectorWorkspace(
                    id: remote.id,
                    alias: remote.localPathAlias,
                    absoluteRoot: workspace.absoluteRoot,
                    fingerprint: remote.localPathFingerprint
                )
            )
        }
        state.workspaces = imported
        try stateStore.save(state)
    }
}

private struct LegacyConnectorState: Decodable {
    var workspaces: [LegacyWorkspace]
}

private struct LegacyWorkspace: Decodable {
    var absoluteRoot: String
    var alias: String
    var fingerprint: String
    enum CodingKeys: String, CodingKey {
        case absoluteRoot = "absolute_root"
        case alias, fingerprint
    }
}
