import Foundation

enum NativePluginArtifactRegistrar {
    private static let maximumArtifactsPerCall = 64
    private static let maximumArtifactBytes = 64 * 1_024 * 1_024
    private static let managedDirectory = "chatos-plugin-artifacts"

    private struct Candidate {
        var producerArtifactID: String
        var displayName: String
        var mediaType: String
        var sizeBytes: Int
        var sha256: String
        var data: Data
    }

    static func register(
        result: NativeJSONValue,
        identity: NativePluginRuntimeStore.Identity,
        ownerUserID: String,
        deviceID: String,
        workspaceID: String?,
        workspaceRootURL: URL?,
        artifactRootURL: URL,
        permissionSnapshot: Set<String>,
        toolName: String
    ) throws -> NativeJSONValue {
        guard var resultObject = result.jsonObject,
              var metadata = resultObject["_meta"]?.jsonObject,
              let rawCandidates = metadata["chatos/artifacts"]?.jsonArray else {
            return result
        }
        guard permissionSnapshot.contains("artifact.create") else {
            throw NativePluginRuntimeError.permissionDenied("Plugin 未获准注册本地产物")
        }
        guard let workspaceID, let workspaceRootURL else {
            throw NativePluginRuntimeError.permissionDenied("Plugin 注册项目产物需要项目工作区")
        }
        guard rawCandidates.count <= maximumArtifactsPerCall else {
            throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP 返回的产物数量超过限制")
        }

        let artifactRoot = try canonicalDirectory(artifactRootURL, label: "Plugin Artifact")
        let workspaceRoot = try canonicalDirectory(workspaceRootURL, label: "项目工作区")
        let candidates = try rawCandidates.map {
            try parseCandidate($0, artifactRoot: artifactRoot)
        }
        var authoritative: [NativeJSONValue] = []
        var createdDirectories: [URL] = []
        do {
            for candidate in candidates {
                let artifactID = "pa_" + UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased()
                let sessionDirectory = safePathComponent(identity.adapterSessionID)
                guard !sessionDirectory.isEmpty else {
                    throw NativePluginRuntimeError.invalidRequest("Plugin Artifact 会话标识无效")
                }
                let parent = try createManagedDirectory(
                    root: workspaceRoot,
                    components: [managedDirectory, sessionDirectory, artifactID]
                )
                createdDirectories.append(parent)
                let destination = parent.appendingPathComponent(candidate.displayName, isDirectory: false)
                try candidate.data.write(to: destination, options: .withoutOverwriting)
                let copied = try Data(contentsOf: destination, options: .mappedIfSafe)
                guard copied.count == candidate.sizeBytes,
                      NativePluginHash.sha256(copied) == candidate.sha256 else {
                    throw NativePluginRuntimeError.invalidMCPResponse("Plugin Artifact 持久化校验失败")
                }
                let relativePath = [managedDirectory, sessionDirectory, artifactID, candidate.displayName]
                    .joined(separator: "/")
                let descriptor: NativeJSONValue = .object([
                    "artifact_id": .string(artifactID),
                    "owner": .object([
                        "owner_user_id": .string(ownerUserID),
                        "run_id": .string(identity.runID),
                        "device_id": .string(deviceID),
                        "workspace_id": .string(workspaceID),
                        "plugin_id": .string(identity.pluginID),
                        "release_id": .string(identity.releaseID),
                        "artifact_sha256": .string(identity.artifactSHA256),
                        "component_key": .string(identity.componentKey),
                        "adapter_session_id": .string(identity.adapterSessionID),
                    ]),
                    "workspace_relative_path": .string(relativePath),
                    "display_name": .string(candidate.displayName),
                    "media_type": .string(candidate.mediaType),
                    "size_bytes": .number(Double(candidate.sizeBytes)),
                    "sha256": .string(candidate.sha256),
                    "created_at": .string(ISO8601DateFormatter().string(from: Date())),
                    "producer_tool_name": .string(toolName),
                    "downloadable": .bool(true),
                    "mutable": .bool(false),
                ])
                authoritative.append(.object([
                    "producer_artifact_id": .string(candidate.producerArtifactID),
                    "artifact": descriptor,
                ]))
            }
        } catch {
            for directory in createdDirectories.reversed() {
                try? FileManager.default.removeItem(at: directory)
            }
            throw error
        }
        metadata["chatos/artifacts"] = .array(authoritative)
        resultObject["_meta"] = .object(metadata)
        return .object(resultObject)
    }

    private static func parseCandidate(
        _ value: NativeJSONValue,
        artifactRoot: URL
    ) throws -> Candidate {
        guard let object = value.jsonObject,
              object.count == 6,
              let producerArtifactID = object["producer_artifact_id"]?.jsonString,
              let relativePath = object["relative_path"]?.jsonString,
              let displayName = object["display_name"]?.jsonString,
              let mediaType = object["media_type"]?.jsonString,
              let sizeNumber = object["size_bytes"]?.jsonNumber,
              let sha256 = object["sha256"]?.jsonString else {
            throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP Artifact 描述无效")
        }
        guard producerArtifactID == producerArtifactID.trimmingCharacters(in: .whitespacesAndNewlines),
              !producerArtifactID.isEmpty,
              producerArtifactID.utf8.count <= 256,
              !producerArtifactID.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
              sizeNumber.isFinite,
              sizeNumber.rounded() == sizeNumber,
              sizeNumber >= 0,
              sizeNumber <= Double(maximumArtifactBytes),
              sha256.count == 64,
              sha256.allSatisfy({ $0.isNumber || ("a"..."f").contains(String($0)) }) else {
            throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP Artifact 标识或大小无效")
        }
        let components = try safeRelativeComponents(relativePath)
        guard displayName == displayName.trimmingCharacters(in: .whitespacesAndNewlines),
              !displayName.isEmpty,
              displayName.utf8.count <= 512,
              !displayName.contains("/"),
              !displayName.contains("\\"),
              !displayName.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains),
              mediaType == mediaType.trimmingCharacters(in: .whitespacesAndNewlines),
              !mediaType.isEmpty,
              mediaType.utf8.count <= 256,
              mediaTypeForPath(displayName) == mediaType else {
            throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP Artifact 文件名或 MIME 类型无效")
        }
        var source = artifactRoot
        for component in components {
            source.appendPathComponent(component, isDirectory: false)
            let values = try source.resourceValues(forKeys: [.isSymbolicLinkKey])
            guard values.isSymbolicLink != true else {
                throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP Artifact 路径包含符号链接")
            }
        }
        let canonicalSource = source.standardizedFileURL.resolvingSymlinksInPath()
        guard isWithin(root: artifactRoot, target: canonicalSource) else {
            throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP Artifact 路径越界")
        }
        let values = try canonicalSource.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        let sizeBytes = Int(sizeNumber)
        guard values.isRegularFile == true, values.fileSize == sizeBytes else {
            throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP Artifact 不是有效文件或大小不匹配")
        }
        let data = try Data(contentsOf: canonicalSource, options: .mappedIfSafe)
        guard data.count == sizeBytes, NativePluginHash.sha256(data) == sha256 else {
            throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP Artifact SHA-256 不匹配")
        }
        return Candidate(
            producerArtifactID: producerArtifactID,
            displayName: displayName,
            mediaType: mediaType,
            sizeBytes: sizeBytes,
            sha256: sha256,
            data: data
        )
    }

    private static func canonicalDirectory(_ url: URL, label: String) throws -> URL {
        let canonical = url.standardizedFileURL.resolvingSymlinksInPath()
        let values = try canonical.resourceValues(forKeys: [.isDirectoryKey])
        guard values.isDirectory == true else {
            throw NativePluginRuntimeError.invalidRequest("\(label)目录不可用")
        }
        return canonical
    }

    private static func safeRelativeComponents(_ path: String) throws -> [String] {
        guard !path.isEmpty,
              path.utf8.count <= 4_096,
              !path.hasPrefix("/"),
              !path.contains("\\"),
              !path.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains) else {
            throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP Artifact 路径无效")
        }
        let components = path.split(separator: "/", omittingEmptySubsequences: false).map(String.init)
        guard !components.contains(where: { $0.isEmpty || $0 == "." || $0 == ".." }) else {
            throw NativePluginRuntimeError.invalidMCPResponse("Plugin MCP Artifact 路径无效")
        }
        return components
    }

    private static func safePathComponent(_ value: String) -> String {
        value.filter { $0.isLetter || $0.isNumber || $0 == "-" || $0 == "_" }
    }

    private static func createManagedDirectory(root: URL, components: [String]) throws -> URL {
        var cursor = root
        for component in components {
            cursor.appendPathComponent(component, isDirectory: true)
            if FileManager.default.fileExists(atPath: cursor.path) {
                let values = try cursor.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
                guard values.isDirectory == true, values.isSymbolicLink != true else {
                    throw NativePluginRuntimeError.invalidRequest("Plugin Artifact 项目目录不安全")
                }
            } else {
                try FileManager.default.createDirectory(at: cursor, withIntermediateDirectories: false)
            }
        }
        return cursor
    }

    private static func isWithin(root: URL, target: URL) -> Bool {
        let rootPath = root.path.hasSuffix("/") ? root.path : root.path + "/"
        return target.path == root.path || target.path.hasPrefix(rootPath)
    }

    private static func mediaTypeForPath(_ path: String) -> String? {
        switch URL(fileURLWithPath: path).pathExtension.lowercased() {
        case "png": "image/png"
        case "jpg", "jpeg": "image/jpeg"
        case "pdf": "application/pdf"
        case "json", "har": "application/json"
        case "txt": "text/plain"
        case "csv": "text/csv"
        case "zip": "application/zip"
        case "docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        case "xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        case "pptx": "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        default: nil
        }
    }
}
