import ChatOSCore
import Foundation

struct NativePluginRelayScope: Sendable {
    let workspace: LocalConnectorWorkspace?

    var workspaceID: String? { workspace?.id }

    static func resolve(
        workspaceID rawWorkspaceID: String,
        workspaces: [LocalConnectorWorkspace]
    ) throws -> Self {
        let workspaceID = rawWorkspaceID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !workspaceID.isEmpty else {
            return .init(workspace: nil)
        }
        guard let workspace = workspaces.first(where: { $0.id == workspaceID }) else {
            throw NativePluginRuntimeError.invalidRequest(
                "Plugin Relay 的工作区未在当前设备注册"
            )
        }
        return .init(workspace: workspace)
    }

    func projectRoot(for request: NativeRelayRequest) throws -> URL? {
        let rawPath = request.header("x-local-connector-project-root")
            ?? request.header("x-local-connector-cwd")
        guard let workspace else {
            guard rawPath == nil else {
                throw NativePluginRuntimeError.invalidRequest(
                    "设备级 Plugin Relay 不得携带项目目录"
                )
            }
            return nil
        }
        return try NativePluginProjectRootResolver.resolve(
            rawPath: rawPath,
            workspace: workspace
        )
    }

    func validate(permissionSnapshot: Set<String>) throws {
        guard workspace == nil else { return }
        let workspacePermissions = permissionSnapshot.filter {
            $0.trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased()
                .hasPrefix("workspace.")
        }
        guard workspacePermissions.isEmpty else {
            throw NativePluginRuntimeError.permissionDenied(
                "设备级 Plugin Relay 不得申请工作区权限："
                    + workspacePermissions.sorted().joined(separator: ", ")
            )
        }
    }
}
