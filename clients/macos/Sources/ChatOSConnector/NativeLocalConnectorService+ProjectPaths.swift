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
            guard deviceID == state.deviceID else {
                throw NativeConnectorError.workspaceUnavailable
            }
            let relative = parts.dropFirst(2).joined(separator: "/")
            guard let workspace = state.workspaces.first(where: { $0.id == workspaceID }) else {
                return try resolveReauthorizedProjectPath(relativePath: relative)
            }
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

    /// Recovers a client-owned project after Local Connector re-pairing replaced the
    /// workspace grant ID. Every candidate is resolved through a current grant and an
    /// existing directory. Distinct physical matches fail closed instead of guessing.
    func resolveReauthorizedProjectPath(relativePath: String) throws -> NativeResolvedProjectPath {
        guard let deviceID = state.deviceID else { throw NativeConnectorError.workspaceUnavailable }
        var candidatesByPath: [String: [NativeResolvedProjectPath]] = [:]
        for workspace in state.workspaces where
            URL(fileURLWithPath: workspace.absoluteRoot).standardizedFileURL.resolvingSymlinksInPath().path == "/" {
            let filesystem = NativeWorkspaceFilesystem(workspace: workspace)
            guard let url = try? filesystem.resolveExistingURL(relativePath.isEmpty ? "." : relativePath),
                  (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true else {
                continue
            }
            let canonical = url.standardizedFileURL.resolvingSymlinksInPath()
            let prefix = "local://connector/\(deviceID)/\(workspace.id)"
            candidatesByPath[canonical.path, default: []].append(.init(
                workspace: workspace,
                relativePath: relativePath.isEmpty ? "." : relativePath,
                absoluteURL: canonical,
                logicalPrefix: prefix
            ))
        }
        guard candidatesByPath.count == 1, let matches = candidatesByPath.values.first else {
            throw NativeConnectorError.workspaceUnavailable
        }
        return matches.sorted {
            if $0.workspace.absoluteRoot.count != $1.workspace.absoluteRoot.count {
                return $0.workspace.absoluteRoot.count > $1.workspace.absoluteRoot.count
            }
            return $0.workspace.id < $1.workspace.id
        }[0]
    }
}
