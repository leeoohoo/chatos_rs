import Foundation

struct NativePreparedPluginLaunch: Sendable {
    var manifest: NativePluginManifest
    var componentKey: String
    var server: NativePluginManifest.MCPServer
    var executableURL: URL
    var arguments: [String]
    var environment: [String: String]
    var installationURL: URL
    var visualSessionURL: URL
    var artifactURL: URL
    var displayName: String
}

enum NativePluginManifestLoader {
    static func prepare(
        record: NativeInstalledPluginRecord,
        componentKey requestedComponentKey: String,
        serverKey: String?,
        adapterSessionID: String,
        ownerUserID: String,
        deviceID: String,
        workspaceID: String? = nil,
        workspaceRoot: URL?,
        projectID: String? = nil,
        projectName: String? = nil,
        permissionSnapshot: Set<String>,
        runtimeRootURL: URL
    ) throws -> NativePreparedPluginLaunch {
        let installationURL = URL(fileURLWithPath: record.installationPath, isDirectory: true)
            .standardizedFileURL
        let manifestURL = installationURL.appendingPathComponent("chatos.plugin.json")
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data(contentsOf: manifestURL, options: .mappedIfSafe)
        )
        guard manifest.schemaVersion == 3,
              manifest.version == record.version else {
            throw NativePluginRuntimeError.invalidManifest("Plugin manifest 与已安装 Release 不一致")
        }
        guard permissionSnapshot.contains("process.spawn") else {
            throw NativePluginRuntimeError.permissionDenied("Plugin 缺少 process.spawn 权限")
        }
        let componentKey = serverKey?.nonEmptyTrimmed ?? requestedComponentKey
        guard let server = manifest.mcpServers[componentKey], server.type == "stdio" else {
            throw NativePluginRuntimeError.invalidManifest("没有找到对应的 stdio MCP 组件")
        }
        let requiredPermissions = manifest.permissions.filter {
            $0.required && ($0.components.isEmpty || $0.components.contains(componentKey))
        }
        guard requiredPermissions.allSatisfy({ permissionSnapshot.contains($0.permission) }) else {
            throw NativePluginRuntimeError.permissionDenied("Plugin 必需权限尚未授权")
        }
        guard Self.safeExecutableName(server.bin) else {
            throw NativePluginRuntimeError.invalidManifest("Plugin 可执行文件名无效")
        }
        let declaredExecutableURL = installationURL
            .appendingPathComponent("bin", isDirectory: true)
            .appendingPathComponent(server.bin, isDirectory: false)
            .standardizedFileURL
        let expectedPrefix = installationURL.path + "/"
        let values = try declaredExecutableURL.resourceValues(forKeys: [
            .isRegularFileKey, .isSymbolicLinkKey, .isExecutableKey,
        ])
        guard declaredExecutableURL.path.hasPrefix(expectedPrefix),
              values.isRegularFile == true,
              values.isSymbolicLink != true,
              values.isExecutable == true else {
            throw NativePluginRuntimeError.invalidManifest("Plugin MCP 可执行文件不可用")
        }
        // Keep the declared launcher for Open Computer Use. Its macOS launcher
        // starts the signed app through LaunchServices and proxies MCP over a
        // local socket, so Accessibility and Screen Recording are evaluated
        // against Open Computer Use.app instead of the ChatOS parent process.
        let executableURL = declaredExecutableURL

        let pluginHash = Self.sha256(record.pluginID)
        let releaseHash = Self.sha256(record.releaseID)
        let sessionHash = Self.sha256(adapterSessionID)
        let userHash = Self.sha256(ownerUserID)
        let visualSessionURL = runtimeRootURL
            .appendingPathComponent("visual-sessions/users", isDirectory: true)
            .appendingPathComponent(userHash, isDirectory: true)
            .appendingPathComponent(pluginHash, isDirectory: true)
            .appendingPathComponent(releaseHash, isDirectory: true)
            .appendingPathComponent(sessionHash, isDirectory: true)
        let runtimeContext = try NativePluginRuntimeContextResolver.resolve(
            manifest: manifest,
            componentKey: componentKey,
            runtimeRootURL: runtimeRootURL,
            pluginID: record.pluginID,
            host: .init(
                ownerUserID: ownerUserID,
                deviceID: deviceID,
                workspaceID: workspaceID,
                workspaceRoot: workspaceRoot,
                projectID: projectID,
                projectName: projectName
            )
        )
        let dataURL = runtimeContext.dataURL
        let cacheURL = runtimeContext.cacheURL
        let artifactURL = runtimeRootURL
            .appendingPathComponent("artifacts/users", isDirectory: true)
            .appendingPathComponent(userHash, isDirectory: true)
            .appendingPathComponent(sessionHash, isDirectory: true)
        let grantURL = runtimeRootURL
            .appendingPathComponent("file-grants/users", isDirectory: true)
            .appendingPathComponent(userHash, isDirectory: true)
            .appendingPathComponent(sessionHash, isDirectory: true)
        for directory in [visualSessionURL, dataURL, cacheURL, artifactURL, grantURL] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }
        let host = NativeJSONValue.object([
            "protocol_version": .number(1),
            "adapter_session_id": .string(adapterSessionID),
            "plugin_id": .string(record.pluginID),
            "component_key": .string(componentKey),
        ])
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(host).write(
            to: visualSessionURL.appendingPathComponent("host.json"),
            options: .atomic
        )

        var environment = server.env
        environment.merge([
            "CHATOS_PLUGIN_ROOT": installationURL.path,
            "CHATOS_PLUGIN_DATA_DIR": dataURL.path,
            "CHATOS_PLUGIN_CACHE_DIR": cacheURL.path,
            "CHATOS_PLUGIN_ARTIFACT_DIR": artifactURL.path,
            "CHATOS_PLUGIN_FILE_GRANT_DIR": grantURL.path,
            "CHATOS_PLUGIN_VISUAL_SESSION_DIR": visualSessionURL.path,
            "CHATOS_PLUGIN_ID": record.pluginID,
            "CHATOS_PLUGIN_COMPONENT_KEY": componentKey,
        ], uniquingKeysWith: { _, runtime in runtime })
        environment.merge(runtimeContext.environment, uniquingKeysWith: { _, runtime in runtime })
#if os(macOS)
        if manifest.name == "open-computer-use", server.bin == "open-computer-use" {
            let managedAppRoot = dataURL
                .appendingPathComponent("managed-app", isDirectory: true)
                .path
            environment["VISUAL_COMPUTER_USE_MANAGED_APP_ROOT"] = managedAppRoot
            environment["OPEN_COMPUTER_USE_MANAGED_APP_ROOT"] = managedAppRoot
        }
#endif
        return .init(
            manifest: manifest,
            componentKey: componentKey,
            server: server,
            executableURL: executableURL,
            arguments: server.args,
            environment: environment,
            installationURL: installationURL,
            visualSessionURL: visualSessionURL,
            artifactURL: artifactURL,
            displayName: manifest.interface?.displayName?.nonEmptyTrimmed ?? manifest.name
        )
    }

    private static func safeExecutableName(_ value: String) -> Bool {
        !value.isEmpty && value.count <= 128 && !value.contains("/") && value != "." && value != ".."
    }

    static func sha256(_ value: String) -> String {
        NativePluginHash.sha256(Data(value.utf8))
    }
}

private extension String {
    var nonEmptyTrimmed: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
