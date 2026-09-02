import Foundation

struct NativePluginHostContext: Sendable, Equatable {
    var ownerUserID: String
    var deviceID: String
    var workspaceID: String?
    var workspaceRoot: URL?
    var projectID: String?
    var projectName: String?
}

struct NativeResolvedPluginRuntimeContext: Sendable, Equatable {
    var dataURL: URL
    var cacheURL: URL
    var environment: [String: String]
    var scopeKey: String

    func websiteDataStoreID(applicationID: String) -> UUID? {
        let digest = NativePluginManifestLoader.sha256("\(applicationID)\n\(scopeKey)")
        guard digest.count >= 32 else { return nil }
        let value = String(digest.prefix(32))
        func segment(_ offset: Int, _ length: Int) -> String {
            let start = value.index(value.startIndex, offsetBy: offset)
            let end = value.index(start, offsetBy: length)
            return String(value[start..<end])
        }
        let uuidString = [
            segment(0, 8),
            segment(8, 4),
            segment(12, 4),
            segment(16, 4),
            segment(20, 12),
        ].joined(separator: "-")
        return UUID(uuidString: uuidString)
    }
}

enum NativePluginRuntimeContextResolver {
    static func resolve(
        manifest: NativePluginManifest,
        componentKey: String,
        runtimeRootURL: URL,
        pluginID: String,
        host: NativePluginHostContext
    ) throws -> NativeResolvedPluginRuntimeContext {
        let pluginHash = NativePluginManifestLoader.sha256(pluginID)
        let userHash = NativePluginManifestLoader.sha256(host.ownerUserID)
        let userPluginDataURL = runtimeRootURL
            .appendingPathComponent("data/users", isDirectory: true)
            .appendingPathComponent(userHash, isDirectory: true)
            .appendingPathComponent(pluginHash, isDirectory: true)
        let userPluginCacheURL = runtimeRootURL
            .appendingPathComponent("cache/users", isDirectory: true)
            .appendingPathComponent(userHash, isDirectory: true)
            .appendingPathComponent(pluginHash, isDirectory: true)

        guard let declaration = manifest.runtimeContext,
              declaration.applies(to: componentKey) else {
            var environment: [String: String] = [:]
            if let workspaceRoot = host.workspaceRoot {
                environment["CHATOS_WORKSPACE"] = workspaceRoot.path
            }
            return .init(
                dataURL: userPluginDataURL,
                cacheURL: userPluginCacheURL,
                environment: environment,
                scopeKey: "user:\(userHash)"
            )
        }

        let requestedFields = Set(declaration.required + declaration.optional)
        let missingRequired = declaration.required.filter { field in
            switch field {
            case "project.id": host.projectID?.pluginContextValue == nil
            case "workspace.id": host.workspaceID?.pluginContextValue == nil
            case "workspace.root": host.workspaceRoot == nil
            default: true
            }
        }
        guard missingRequired.isEmpty else {
            throw NativePluginRuntimeError.invalidRequest(
                "Plugin 运行缺少必需上下文：\(missingRequired.sorted().joined(separator: ", "))"
            )
        }

        let resolvedScope = try resolvedScope(declaration: declaration, host: host)
        let scopeHash = NativePluginManifestLoader.sha256(resolvedScope.identity)
        let storageScope = try storageScope(
            declaration: declaration,
            resolvedScope: resolvedScope,
            host: host
        )
        let storageHash = NativePluginManifestLoader.sha256(storageScope.identity)
        let dataURL = declaration.storageIsolation == "plugin"
            ? userPluginDataURL
            : userPluginDataURL
                .appendingPathComponent("scopes/\(storageScope.kind)", isDirectory: true)
                .appendingPathComponent(storageHash, isDirectory: true)
        let cacheURL = declaration.storageIsolation == "plugin"
            ? userPluginCacheURL
            : userPluginCacheURL
                .appendingPathComponent("scopes/\(storageScope.kind)", isDirectory: true)
                .appendingPathComponent(storageHash, isDirectory: true)

        var environment = [
            "CHATOS_CONTEXT_SCOPE": resolvedScope.kind,
            "CHATOS_CONTEXT_SCOPE_ID": scopeHash,
        ]
        if requestedFields.contains("project.id"), let projectID = host.projectID?.pluginContextValue {
            environment["CHATOS_PROJECT_ID"] = projectID
        }
        if requestedFields.contains("workspace.id"), let workspaceID = host.workspaceID?.pluginContextValue {
            environment["CHATOS_WORKSPACE_ID"] = workspaceID
        }
        if requestedFields.contains("workspace.root"), let workspaceRoot = host.workspaceRoot {
            environment["CHATOS_WORKSPACE"] = workspaceRoot.path
        }
        if let projectName = host.projectName?.pluginContextValue {
            environment["CHATOS_PROJECT_NAME"] = projectName
        }
        return .init(
            dataURL: dataURL,
            cacheURL: cacheURL,
            environment: environment,
            scopeKey: "user:\(userHash):\(resolvedScope.kind):\(scopeHash)"
        )
    }

    private static func resolvedScope(
        declaration: NativePluginManifest.RuntimeContext,
        host: NativePluginHostContext
    ) throws -> (kind: String, identity: String) {
        switch declaration.scope {
        case "device":
            return ("device", "device:\(host.deviceID)")
        case "workspace":
            if let workspaceID = host.workspaceID?.pluginContextValue {
                return ("workspace", "workspace:\(workspaceID)")
            }
        case "project":
            if let projectID = host.projectID?.pluginContextValue {
                return ("project", "project:\(projectID)")
            }
        default:
            throw NativePluginRuntimeError.invalidManifest("Plugin runtimeContext.scope 无效")
        }
        if declaration.missingContext == "device" {
            return ("device", "device:\(host.deviceID)")
        }
        throw NativePluginRuntimeError.invalidRequest("Plugin 需要项目或工作区上下文")
    }

    private static func storageScope(
        declaration: NativePluginManifest.RuntimeContext,
        resolvedScope: (kind: String, identity: String),
        host: NativePluginHostContext
    ) throws -> (kind: String, identity: String) {
        switch declaration.storageIsolation {
        case "plugin":
            return ("plugin", "plugin")
        case "workspace":
            if let workspaceID = host.workspaceID?.pluginContextValue {
                return ("workspace", "workspace:\(workspaceID)")
            }
        case "project":
            if let projectID = host.projectID?.pluginContextValue {
                return ("project", "project:\(projectID)")
            }
        default:
            throw NativePluginRuntimeError.invalidManifest(
                "Plugin runtimeContext.storageIsolation 无效"
            )
        }
        if declaration.missingContext == "device" {
            return ("device", "device:\(host.deviceID)")
        }
        return resolvedScope
    }
}

extension String {
    var pluginContextValue: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
