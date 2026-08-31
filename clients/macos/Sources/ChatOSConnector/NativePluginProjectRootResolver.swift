import ChatOSCore
import Foundation

enum NativePluginProjectRootResolver {
    static func resolve(
        rawPath: String?,
        workspace: LocalConnectorWorkspace
    ) throws -> URL {
        let workspaceRoot = URL(fileURLWithPath: workspace.absoluteRoot, isDirectory: true)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        let normalizedPath = rawPath?.trimmingCharacters(in: .whitespacesAndNewlines)
        let candidate: URL
        if let normalizedPath, !normalizedPath.isEmpty, normalizedPath != "." {
            candidate = normalizedPath.hasPrefix("/")
                ? URL(fileURLWithPath: normalizedPath, isDirectory: true)
                : workspaceRoot.appendingPathComponent(normalizedPath, isDirectory: true)
        } else {
            candidate = workspaceRoot
        }
        let resolved = candidate.standardizedFileURL.resolvingSymlinksInPath()
        let workspacePrefix = workspaceRoot.path.hasSuffix("/")
            ? workspaceRoot.path
            : workspaceRoot.path + "/"
        var isDirectory: ObjCBool = false
        guard resolved.path == workspaceRoot.path || resolved.path.hasPrefix(workspacePrefix),
              FileManager.default.fileExists(atPath: resolved.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw NativePluginRuntimeError.invalidRequest("Plugin 项目目录不在已授权工作区内或不可用")
        }
        return resolved
    }
}
