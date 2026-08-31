import ChatOSCore
import Foundation

struct NativeResolvedProjectPath: Sendable {
    var workspace: LocalConnectorWorkspace
    var relativePath: String
    var absoluteURL: URL
    var logicalPrefix: String?

    func logicalPath(for relativePath: String) -> String {
        guard let logicalPrefix else {
            if relativePath == "." { return absoluteURL.path }
            let root = workspace.absoluteRoot.hasSuffix("/")
                ? String(workspace.absoluteRoot.dropLast())
                : workspace.absoluteRoot
            return root + "/" + relativePath
        }
        return relativePath == "." ? logicalPrefix : logicalPrefix + "/" + relativePath
    }
}

extension NativeLocalConnectorService {
    func resolveProjectPath(_ rawPath: String) throws -> NativeResolvedProjectPath {
        let value = rawPath.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else { throw NativeConnectorError.workspaceUnavailable }

        if let components = URLComponents(string: value),
           components.scheme?.lowercased() == "local",
           components.host?.lowercased() == "connector" {
            let parts = components.path.split(separator: "/", omittingEmptySubsequences: true).map(String.init)
            guard parts.count >= 2 else { throw NativeConnectorError.workspaceUnavailable }
            let deviceID = parts[0]
            let workspaceID = parts[1]
            guard deviceID == state.deviceID,
                  let workspace = state.workspaces.first(where: { $0.id == workspaceID }) else {
                throw NativeConnectorError.workspaceUnavailable
            }
            let relative = parts.dropFirst(2).joined(separator: "/")
            let filesystem = NativeWorkspaceFilesystem(workspace: workspace)
            let absoluteURL = try filesystem.resolveExistingURL(relative.isEmpty ? "." : relative)
            let prefix = "local://connector/\(deviceID)/\(workspaceID)"
            return .init(
                workspace: workspace,
                relativePath: relative.isEmpty ? "." : relative,
                absoluteURL: absoluteURL,
                logicalPrefix: prefix
            )
        }

        let candidate = URL(fileURLWithPath: value).standardizedFileURL.resolvingSymlinksInPath()
        guard let workspace = state.workspaces.first(where: { workspace in
            let root = URL(fileURLWithPath: workspace.absoluteRoot).standardizedFileURL.resolvingSymlinksInPath()
            let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
            return candidate.path == root.path || candidate.path.hasPrefix(prefix)
        }) else {
            throw NativeConnectorError.workspaceUnavailable
        }
        let root = URL(fileURLWithPath: workspace.absoluteRoot).standardizedFileURL.resolvingSymlinksInPath()
        let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
        let relative = candidate.path == root.path ? "." : String(candidate.path.dropFirst(prefix.count))
        return .init(
            workspace: workspace,
            relativePath: relative,
            absoluteURL: candidate,
            logicalPrefix: nil
        )
    }
}
