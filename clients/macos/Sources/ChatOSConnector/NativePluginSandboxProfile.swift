import Darwin
import Foundation

enum NativePluginSandboxProfile {
    static let executableURL = URL(fileURLWithPath: "/usr/bin/sandbox-exec")

    static func arguments(for launch: NativePreparedPluginLaunch) throws -> [String] {
        guard FileManager.default.isExecutableFile(atPath: executableURL.path) else {
            throw NativePluginRuntimeError.permissionDenied(
                "当前 macOS 无法提供 Plugin 进程隔离"
            )
        }
        let profile = try profile(for: launch)
        return ["-p", profile, launch.executableURL.path] + launch.arguments
    }

    private static func profile(for launch: NativePreparedPluginLaunch) throws -> String {
        var rules = [
            "(version 1)",
            "(deny default)",
            "(import \"system.sb\")",
            "(allow process-exec (subpath \"/System\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/opt/homebrew\") (subpath \"/usr/local\") \(literal(launch.installationURL.path)))",
            "(allow process-fork)",
            "(allow signal (target same-sandbox))",
            "(allow sysctl-read)",
            "(allow mach-lookup)",
            "(allow file-read-metadata)",
            "(allow file-read* (subpath \"/System\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/opt/homebrew\") (subpath \"/usr/local\") (subpath \"/Library/Apple\") \(literal(launch.installationURL.path)))",
        ]
        let writableKeys = [
            "CHATOS_PLUGIN_DATA_DIR", "CHATOS_PLUGIN_CACHE_DIR", "CHATOS_PLUGIN_ARTIFACT_DIR",
            "CHATOS_PLUGIN_FILE_GRANT_DIR", "CHATOS_PLUGIN_VISUAL_SESSION_DIR",
        ]
        for path in writableKeys.compactMap({ launch.environment[$0] }) {
            rules.append("(allow file-read* file-write* \(literal(path)))")
        }
        if let workspace = launch.workspaceRoot?.standardizedFileURL.resolvingSymlinksInPath().path {
            if launch.permissionSnapshot.contains("workspace.write") {
                rules.append("(allow file-read* file-write* \(literal(workspace)))")
            } else if launch.permissionSnapshot.contains("workspace.read") {
                rules.append("(allow file-read* \(literal(workspace)))")
            }
        }
        // Direct sockets remain denied. Until a host-controlled domain proxy is configured,
        // network permissions fail closed instead of granting the child unrestricted sockets.
        return rules.joined(separator: "\n")
    }

    private static func literal(_ path: String) -> String {
        var resolved = [CChar](repeating: 0, count: Int(PATH_MAX))
        let canonical = path.withCString { pointer -> String in
            guard realpath(pointer, &resolved) != nil else {
                return URL(fileURLWithPath: path).standardizedFileURL.path
            }
            return String(
                decoding: resolved.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) },
                as: UTF8.self
            )
        }
        let escaped = canonical
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        return "(subpath \"\(escaped)\")"
    }
}
