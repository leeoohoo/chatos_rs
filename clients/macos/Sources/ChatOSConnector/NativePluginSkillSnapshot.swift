import Foundation

enum NativePluginSkillSnapshotLoader {
    private static let maximumInstructionsBytes = 256 * 1024
    private static let maximumResourceBytes = 1024 * 1024
    private static let maximumTotalResourceBytes = 4 * 1024 * 1024
    private static let maximumResourceCount = 256

    static func prepareBody(
        record: NativeInstalledPluginRecord,
        componentKey: String,
        skillKeys: [String],
        expectedContentSHA256: String?,
        runID: String,
        adapterSessionID: String,
        now: Date = Date(),
        fileManager: FileManager = .default
    ) throws -> NativeJSONValue {
        guard expectedContentSHA256 == nil
                || expectedContentSHA256 == record.artifactSHA256 else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Skill 组件快照与已安装 Release 不匹配")
        }
        let installationURL = URL(fileURLWithPath: record.installationPath, isDirectory: true)
            .standardizedFileURL
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data(
                contentsOf: installationURL.appendingPathComponent("chatos.plugin.json"),
                options: .mappedIfSafe
            )
        )
        guard manifest.schemaVersion == 3, manifest.version == record.version else {
            throw NativePluginRuntimeError.invalidManifest("Plugin manifest 与已安装 Release 不一致")
        }
        let matchingSkill = manifest.skills.enumerated().first { index, skill in
            componentKeyFromPath(skill.path, fallback: "skills", index: index) == componentKey
        }?.element
        guard let matchingSkill else {
            throw NativePluginRuntimeError.invalidManifest("没有找到对应的 Plugin Skill 组件")
        }
        let relativeCollectionPath = try normalizedRelativePath(matchingSkill.path)
        let collectionURL = installationURL
            .appendingPathComponent(relativeCollectionPath, isDirectory: true)
            .standardizedFileURL
        try validateDirectory(collectionURL, beneath: installationURL, fileManager: fileManager)
        let skillURL = collectionURL.appendingPathComponent("SKILL.md", isDirectory: false)
        let skillData = try readRegularFile(
            skillURL,
            beneath: installationURL,
            maximumBytes: maximumInstructionsBytes,
            fileManager: fileManager
        )
        guard var instructions = String(data: skillData, encoding: .utf8) else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 指令不是 UTF-8 文本")
        }
        instructions = instructions.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !instructions.isEmpty else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 指令为空")
        }
        let fallbackName = collectionURL.lastPathComponent
        let metadata = try parseFrontmatter(instructions, fallbackName: fallbackName)
        let normalizedSkillKeys = Array(Set(skillKeys.map {
            $0.trimmingCharacters(in: .whitespacesAndNewlines)
        }.filter { !$0.isEmpty })).sorted()
        guard normalizedSkillKeys == [metadata.name], metadata.name == componentKey else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Skill 选择与组件身份不匹配")
        }

        let resources = try resourceDescriptors(
            collectionURL: collectionURL,
            installationURL: installationURL,
            fileManager: fileManager
        )
        let relativeSkillPath = relativeCollectionPath + "/SKILL.md"
        let instructionsSHA256 = NativePluginHash.sha256(Data(instructions.utf8))
        var snapshotPayload = """
        chatos.plugin.skill.snapshot.v1
        \(record.pluginID)
        \(record.releaseID)
        \(record.version)
        \(record.artifactSHA256)
        \(componentKey)
        \(relativeSkillPath)
        \(instructionsSHA256)
        """
        for resource in resources {
            let object = try resource.requireObject()
            snapshotPayload += "\n\(try object.requireString("relative_path")):\(try object.requireString("sha256"))"
        }
        let snapshotSHA256 = NativePluginHash.sha256(Data(snapshotPayload.utf8))
        let skillSnapshot: NativeJSONValue = .object([
            "plugin_id": .string(record.pluginID),
            "release_id": .string(record.releaseID),
            "version": .string(record.version),
            "artifact_sha256": .string(record.artifactSHA256),
            "component_key": .string(componentKey),
            "skill_key": .string(metadata.name),
            "relative_skill_path": .string(relativeSkillPath),
            "instructions_sha256": .string(instructionsSHA256),
            "snapshot_sha256": .string(snapshotSHA256),
            "metadata": .object([
                "name": .string(metadata.name),
                "description": metadata.description.map(NativeJSONValue.string) ?? .null,
                "disable_model_invocation": .bool(metadata.disableModelInvocation),
            ]),
            "instructions": .string(instructions),
            "resources": .array(resources),
        ])
        let sessionSHA256 = try NativePluginHash.canonicalSHA256(.object([
            "run_id": .string(runID),
            "adapter_session_id": .string(adapterSessionID),
            "plugin_id": .string(record.pluginID),
            "release_id": .string(record.releaseID),
            "component_key": .string(componentKey),
            "snapshot_sha256": .string(snapshotSHA256),
        ]))
        let expiresAt = Int(now.addingTimeInterval(8 * 24 * 60 * 60).timeIntervalSince1970)
        return .object([
            "run_id": .string(runID),
            "plugin_id": .string(record.pluginID),
            "release_id": .string(record.releaseID),
            "version": .string(record.version),
            "artifact_sha256": .string(record.artifactSHA256),
            "component_key": .string(componentKey),
            "skills": .array([skillSnapshot]),
            "commands": .array([]),
            "agents": .array([]),
            "operations": .array([.string("load_skill_resource")]),
            "adapter_session_id": .string(adapterSessionID),
            "session_sha256": .string(sessionSHA256),
            "expires_at": .number(Double(expiresAt)),
        ])
    }

    private struct SkillMetadata {
        var name: String
        var description: String?
        var disableModelInvocation: Bool
    }

    private static func parseFrontmatter(
        _ instructions: String,
        fallbackName: String
    ) throws -> SkillMetadata {
        var metadata = SkillMetadata(
            name: fallbackName,
            description: nil,
            disableModelInvocation: false
        )
        let normalized = instructions.replacingOccurrences(of: "\r\n", with: "\n")
        if normalized.hasPrefix("---\n"),
           let end = normalized.dropFirst(4).range(of: "\n---\n") {
            let frontmatter = normalized.dropFirst(4)[..<end.lowerBound]
            for rawLine in frontmatter.split(separator: "\n", omittingEmptySubsequences: false) {
                let line = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !line.isEmpty, !line.hasPrefix("#"),
                      let separator = line.firstIndex(of: ":") else { continue }
                let key = line[..<separator].trimmingCharacters(in: .whitespacesAndNewlines)
                var value = line[line.index(after: separator)...]
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                if value.count >= 2,
                   (value.hasPrefix("\"") && value.hasSuffix("\""))
                    || (value.hasPrefix("'") && value.hasSuffix("'")) {
                    value.removeFirst()
                    value.removeLast()
                }
                switch key {
                case "name" where !value.isEmpty:
                    metadata.name = value
                case "description" where !value.isEmpty:
                    metadata.description = value
                case "disable-model-invocation":
                    guard value == "true" || value == "false" || value.isEmpty else {
                        throw NativePluginRuntimeError.invalidManifest(
                            "Plugin Skill disable-model-invocation 必须是布尔值"
                        )
                    }
                    metadata.disableModelInvocation = value == "true"
                default:
                    break
                }
            }
        }
        guard !metadata.name.isEmpty,
              metadata.name.count <= 128,
              metadata.name.utf8.allSatisfy({ byte in
                  byte.isLowercaseASCII || byte.isNumberASCII || byte == 45 || byte == 95
              }) else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill name 格式无效")
        }
        guard metadata.description?.utf8.count ?? 0 <= 4096 else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill description 过长")
        }
        return metadata
    }

    private static func resourceDescriptors(
        collectionURL: URL,
        installationURL: URL,
        fileManager: FileManager
    ) throws -> [NativeJSONValue] {
        guard let enumerator = fileManager.enumerator(
            at: collectionURL,
            includingPropertiesForKeys: [.isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey],
            options: [.skipsHiddenFiles]
        ) else {
            throw NativePluginRuntimeError.invalidManifest("无法读取 Plugin Skill 资源")
        }
        var totalBytes = 0
        var resources: [NativeJSONValue] = []
        for case let fileURL as URL in enumerator {
            if fileURL.lastPathComponent == "SKILL.md" { continue }
            let values = try fileURL.resourceValues(forKeys: [
                .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
            ])
            guard values.isRegularFile == true, values.isSymbolicLink != true else { continue }
            guard resources.count < maximumResourceCount else {
                throw NativePluginRuntimeError.invalidManifest("Plugin Skill 资源数量过多")
            }
            let data = try readRegularFile(
                fileURL,
                beneath: installationURL,
                maximumBytes: maximumResourceBytes,
                fileManager: fileManager
            )
            totalBytes += data.count
            guard totalBytes <= maximumTotalResourceBytes else {
                throw NativePluginRuntimeError.invalidManifest("Plugin Skill 资源总大小过大")
            }
            let relativePath = String(
                fileURL.standardizedFileURL.path.dropFirst(installationURL.path.count + 1)
            )
            resources.append(.object([
                "relative_path": .string(relativePath),
                "sha256": .string(NativePluginHash.sha256(data)),
                "size_bytes": .number(Double(data.count)),
                "kind": .string(resourceKind(relativePath)),
            ]))
        }
        return resources.sorted {
            ($0.jsonObject?["relative_path"]?.jsonString ?? "")
                < ($1.jsonObject?["relative_path"]?.jsonString ?? "")
        }
    }

    private static func resourceKind(_ relativePath: String) -> String {
        let root = relativePath.split(separator: "/").first.map(String.init) ?? ""
        switch root {
        case "skills": return relativePath.hasSuffix("/SKILL.md") ? "skill_instructions" : "reference"
        case "references": return "reference"
        case "scripts": return "script"
        case "assets": return "asset"
        case "schemas": return "schema"
        case "binaries": return "binary"
        case "licenses": return "license"
        default: return "other_text"
        }
    }

    private static func validateDirectory(
        _ directoryURL: URL,
        beneath installationURL: URL,
        fileManager: FileManager
    ) throws {
        guard directoryURL.path.hasPrefix(installationURL.path + "/"),
              fileManager.fileExists(atPath: directoryURL.path) else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 目录不存在")
        }
        let values = try directoryURL.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
        guard values.isDirectory == true, values.isSymbolicLink != true else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 目录不可用")
        }
    }

    private static func readRegularFile(
        _ fileURL: URL,
        beneath installationURL: URL,
        maximumBytes: Int,
        fileManager: FileManager
    ) throws -> Data {
        let standardized = fileURL.standardizedFileURL
        guard standardized.path.hasPrefix(installationURL.path + "/"),
              fileManager.fileExists(atPath: standardized.path) else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 资源不存在")
        }
        let values = try standardized.resourceValues(forKeys: [
            .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
        ])
        guard values.isRegularFile == true,
              values.isSymbolicLink != true,
              values.fileSize ?? maximumBytes + 1 <= maximumBytes else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 资源不可用或过大")
        }
        let data = try Data(contentsOf: standardized, options: .mappedIfSafe)
        guard data.count <= maximumBytes else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 资源过大")
        }
        return data
    }

    private static func normalizedRelativePath(_ value: String) throws -> String {
        var path = value.trimmingCharacters(in: .whitespacesAndNewlines)
        if path.hasPrefix("./") { path.removeFirst(2) }
        let segments = path.split(separator: "/", omittingEmptySubsequences: false)
        guard !path.isEmpty, !path.hasPrefix("/"), segments.allSatisfy({
            !$0.isEmpty && $0 != "." && $0 != ".."
        }) else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 路径无效")
        }
        return path
    }

    private static func componentKeyFromPath(_ path: String, fallback: String, index: Int) -> String {
        let candidate = path
            .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            .split(separator: "/")
            .last
            .map(String.init)?
            .split(separator: ".")
            .first
            .map(String.init) ?? fallback
        var normalized = candidate.lowercased().map { character -> Character in
            character.isASCII && (character.isLetter || character.isNumber) ? character : "-"
        }
        while String(normalized).contains("--") {
            normalized = Array(String(normalized).replacingOccurrences(of: "--", with: "-"))
        }
        var value = String(normalized).trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        if value.isEmpty { value = fallback }
        return index > 0 && value == fallback ? "\(value)-\(index + 1)" : value
    }
}

private extension UInt8 {
    var isLowercaseASCII: Bool { self >= 97 && self <= 122 }
    var isNumberASCII: Bool { self >= 48 && self <= 57 }
}
