import Darwin
import Foundation

enum NativePluginNetworkAccess: Sendable, Equatable {
    case disabled
    case loopbackServer
}

struct NativePluginProcessLaunch: Sendable {
    var record: NativeInstalledPluginRecord
    var executableURL: URL
    var arguments: [String]
    var environment: [String: String]
    var installationURL: URL
    var writableDirectories: [URL]
    var executableDirectories: [URL]
    var workspaceRoot: URL?
    var permissionSnapshot: Set<String>
    var networkAccess: NativePluginNetworkAccess
    var homeDirectory: URL?

    init(
        record: NativeInstalledPluginRecord,
        executableURL: URL,
        arguments: [String],
        environment: [String: String],
        installationURL: URL,
        writableDirectories: [URL],
        executableDirectories: [URL] = [],
        workspaceRoot: URL?,
        permissionSnapshot: Set<String>,
        networkAccess: NativePluginNetworkAccess,
        homeDirectory: URL? = nil
    ) {
        self.record = record
        self.executableURL = executableURL
        self.arguments = arguments
        self.environment = environment
        self.installationURL = installationURL
        self.writableDirectories = writableDirectories
        self.executableDirectories = executableDirectories
        self.workspaceRoot = workspaceRoot
        self.permissionSnapshot = permissionSnapshot
        self.networkAccess = networkAccess
        self.homeDirectory = homeDirectory
    }

    init(stdio launch: NativePreparedPluginLaunch) {
        let writableKeys = [
            "CHATOS_PLUGIN_DATA_DIR", "CHATOS_PLUGIN_CACHE_DIR", "CHATOS_PLUGIN_ARTIFACT_DIR",
            "CHATOS_PLUGIN_FILE_GRANT_DIR", "CHATOS_PLUGIN_VISUAL_SESSION_DIR",
        ]
        var writableDirectories = writableKeys.compactMap { launch.environment[$0] }
            .map { URL(fileURLWithPath: $0, isDirectory: true) }
        var executableDirectories = [
            launch.environment["VISUAL_COMPUTER_USE_MANAGED_APP_ROOT"],
            launch.environment["OPEN_COMPUTER_USE_MANAGED_APP_ROOT"],
        ].compactMap { $0 }
            .map { URL(fileURLWithPath: $0, isDirectory: true) }
        let isBrowserPlugin = launch.record.pluginKey == NativeBrowserPluginIdentity.marketplaceKey
            && launch.manifest.name == NativeBrowserPluginIdentity.packageName
            && launch.componentKey == NativeBrowserPluginIdentity.componentKey
        let isWebDesignPlugin = launch.record.pluginKey
            == NativeWebDesignPluginIdentity.marketplaceKey
            && launch.manifest.name == NativeWebDesignPluginIdentity.packageName
            && launch.componentKey == NativeWebDesignPluginIdentity.componentKey
        let isDocumentPlugin = launch.record.pluginKey
            == NativeDocumentPluginIdentity.marketplaceKey
            && launch.manifest.name == NativeDocumentPluginIdentity.packageName
            && launch.componentKey == NativeDocumentPluginIdentity.componentKey
        let requiresManagedBrowser = isBrowserPlugin || isWebDesignPlugin || isDocumentPlugin
        let requiresLoopbackServer = isBrowserPlugin || isWebDesignPlugin
        let browserHome = isBrowserPlugin
            && launch.permissionSnapshot.contains("browser.chrome.attach")
            ? FileManager.default.homeDirectoryForCurrentUser
            : nil
        if let browserHome {
            writableDirectories.append(contentsOf: Self.browserBridgeWritableDirectories(
                homeDirectory: browserHome
            ))
        }
        if requiresManagedBrowser {
            executableDirectories.append(contentsOf: Self.installedBrowserApplications())
        }
        self.init(
            record: launch.record,
            executableURL: launch.executableURL,
            arguments: launch.arguments,
            environment: launch.environment,
            installationURL: launch.installationURL,
            writableDirectories: writableDirectories,
            executableDirectories: executableDirectories,
            workspaceRoot: launch.workspaceRoot,
            permissionSnapshot: launch.permissionSnapshot,
            networkAccess: requiresLoopbackServer ? .loopbackServer : .disabled,
            homeDirectory: browserHome
        )
    }

    private static func browserBridgeWritableDirectories(
        homeDirectory: URL
    ) -> [URL] {
        let applicationSupport = homeDirectory
            .appendingPathComponent("Library/Application Support", isDirectory: true)
        return [
            applicationSupport.appendingPathComponent("Chatos/browser-bridge", isDirectory: true),
            applicationSupport.appendingPathComponent(
                "Google/Chrome/NativeMessagingHosts",
                isDirectory: true
            ),
            applicationSupport.appendingPathComponent(
                "Chromium/NativeMessagingHosts",
                isDirectory: true
            ),
            applicationSupport.appendingPathComponent(
                "Microsoft Edge/NativeMessagingHosts",
                isDirectory: true
            ),
        ]
    }

    private static func installedBrowserApplications() -> [URL] {
        [
            "/Applications/Google Chrome.app",
            "/Applications/Chromium.app",
            "/Applications/Microsoft Edge.app",
        ].compactMap { path in
            guard FileManager.default.fileExists(atPath: path) else { return nil }
            return URL(fileURLWithPath: path, isDirectory: true)
        }
    }
}

struct NativeSandboxedPluginProcess: Sendable {
    var executableURL: URL
    var arguments: [String]
    var environment: [String: String]
}

enum NativePluginProcessLauncher {
    static func prepare(_ launch: NativePluginProcessLaunch) throws -> NativeSandboxedPluginProcess {
        try NativePluginInstallationIntegrity.verify(
            record: launch.record,
            installationURL: launch.installationURL
        )
        for directory in launch.writableDirectories {
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true
            )
        }
        let sandboxExecutable = NativePluginSandboxProfile.executableURL
        return .init(
            executableURL: sandboxExecutable,
            arguments: [sandboxExecutable.path]
                + (try NativePluginSandboxProfile.arguments(for: launch)),
            environment: NativePluginProcessEnvironment.make(
                overrides: launch.environment,
                homeDirectory: launch.homeDirectory?.path
            )
        )
    }
}

enum NativePluginSandboxProfile {
    static let executableURL = URL(fileURLWithPath: "/usr/bin/sandbox-exec")

    static func arguments(for launch: NativePluginProcessLaunch) throws -> [String] {
        guard FileManager.default.isExecutableFile(atPath: executableURL.path) else {
            throw NativePluginRuntimeError.permissionDenied(
                "当前 macOS 无法提供 Plugin 进程隔离"
            )
        }
        let profile = try profile(for: launch)
        return ["-p", profile, launch.executableURL.path] + launch.arguments
    }

    private static func profile(for launch: NativePluginProcessLaunch) throws -> String {
        var rules = [
            "(version 1)",
            "(deny default)",
            "(import \"system.sb\")",
            "(allow process-exec (subpath \"/System\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/opt/homebrew\") (subpath \"/usr/local\") \(literal(launch.installationURL.path)))",
            "(allow process-fork)",
            "(allow signal (target same-sandbox))",
            "(allow sysctl-read)",
            "(allow mach-lookup (global-name \"com.apple.cfprefsd.agent\") (global-name \"com.apple.system.logger\") (global-name \"com.apple.system.notification_center\") (global-name \"com.apple.securityd\") (global-name \"com.apple.system.opendirectoryd.libinfo\"))",
            "(allow file-read-metadata)",
            "(allow file-read* (subpath \"/System\") (subpath \"/usr\") (subpath \"/bin\") (subpath \"/opt/homebrew\") (subpath \"/usr/local\") (subpath \"/Library/Apple\") \(literal(launch.installationURL.path)))",
        ]
        for directory in launch.writableDirectories {
            rules.append("(allow file-read* file-write* \(literal(directory.path)))")
        }
        for directory in launch.executableDirectories {
            rules.append("(allow file-read* \(literal(directory.path)))")
            rules.append("(allow process-exec \(literal(directory.path)))")
        }
        let computerPermissions: Set<String> = [
            "computer.control", "computer.accessibility", "computer.screen-recording",
        ]
        if !launch.permissionSnapshot.isDisjoint(with: computerPermissions) {
            rules.append("(allow mach-lookup (global-name \"com.apple.tccd\") (global-name \"com.apple.tccd.system\") (global-name \"com.apple.windowserver.active\") (global-name \"com.apple.WindowServer\"))")
        }
        if let workspace = launch.workspaceRoot?.standardizedFileURL.resolvingSymlinksInPath().path {
            if launch.permissionSnapshot.contains("workspace.write") {
                rules.append("(allow file-read* file-write* \(literal(workspace)))")
            } else if launch.permissionSnapshot.contains("workspace.read") {
                rules.append("(allow file-read* \(literal(workspace)))")
            }
        }
        if launch.networkAccess == .loopbackServer {
            rules.append("(allow network-inbound (local ip \"localhost:*\"))")
            rules.append("(allow network-outbound (remote ip \"localhost:*\"))")
        }
        // All non-loopback sockets remain denied. Domain access requires a host-controlled proxy.
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
