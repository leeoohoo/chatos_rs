import Foundation

enum NativeLocalAgentExecutionDirectoryResolver {
    static func resolve(_ path: String, projectRoot: URL) throws -> URL {
        let root = projectRoot.standardizedFileURL.resolvingSymlinksInPath()
        let candidate = path.hasPrefix("/")
            ? URL(fileURLWithPath: path)
            : root.appendingPathComponent(path)
        let resolved = candidate.standardizedFileURL.resolvingSymlinksInPath()
        let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
        var isDirectory: ObjCBool = false
        guard (resolved.path == root.path || resolved.path.hasPrefix(prefix)),
              FileManager.default.fileExists(atPath: resolved.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw NativeLocalAgentPlatformToolError.invalidField("path")
        }
        return resolved
    }
}
