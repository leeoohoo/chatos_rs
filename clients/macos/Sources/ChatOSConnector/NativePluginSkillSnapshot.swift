import Foundation

enum NativePluginSkillSnapshotLoader {
    static let protocolVersion = 2
    private static let maximumInstructionsBytes = 256 * 1024
    private static let maximumResourceBytes = 1024 * 1024
    private static let maximumTotalResourceBytes = 4 * 1024 * 1024
    private static let maximumResourceCount = 256

    static func prepareV2Body(
        record: NativeInstalledPluginRecord,
        componentKey: String,
        expectedSnapshot: NativeJSONValue,
        runID: String,
        adapterSessionID: String,
        now: Date = Date(),
        fileManager: FileManager = .default
    ) throws -> NativeJSONValue {
        let validated = try validatedV2Snapshot(
            record: record,
            componentKey: componentKey,
            expectedSnapshot: expectedSnapshot,
            fileManager: fileManager
        )
        let sessionSHA256 = try NativePluginHash.canonicalSHA256(.object([
            "protocol_version": .number(Double(protocolVersion)),
            "run_id": .string(runID),
            "adapter_session_id": .string(adapterSessionID),
            "plugin_id": .string(record.pluginID),
            "release_id": .string(record.releaseID),
            "component_key": .string(componentKey),
            "snapshot_sha256": .string(validated.snapshotSHA256),
        ]))
        return .object([
            "protocol_version": .number(Double(protocolVersion)),
            "run_id": .string(runID),
            "plugin_id": .string(record.pluginID),
            "release_id": .string(record.releaseID),
            "version": .string(record.version),
            "artifact_sha256": .string(record.artifactSHA256),
            "component_key": .string(componentKey),
            "skills": .array([validated.catalogSnapshot]),
            "commands": .array([]),
            "agents": .array([]),
            "operations": .array([
                .string("skill_activate"),
                .string("skill_read_resource"),
            ]),
            "adapter_session_id": .string(adapterSessionID),
            "session_sha256": .string(sessionSHA256),
            "expires_at": .number(Double(Int(now.addingTimeInterval(8 * 24 * 60 * 60).timeIntervalSince1970))),
        ])
    }

    static func activateV2(
        record: NativeInstalledPluginRecord,
        componentKey: String,
        expectedSnapshot: NativeJSONValue,
        fileManager: FileManager = .default
    ) throws -> NativeJSONValue {
        let validated = try validatedV2Snapshot(
            record: record,
            componentKey: componentKey,
            expectedSnapshot: expectedSnapshot,
            fileManager: fileManager
        )
        guard let instructions = String(data: validated.skillData, encoding: .utf8) else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 指令不是 UTF-8 文本")
        }
        return .object([
            "skill_id": .string(componentKey),
            "instructions": .string(instructions),
            "instructions_sha256": .string(validated.instructionsSHA256),
            "resource_manifest_sha256": .string(validated.resourceManifestSHA256),
            "snapshot_sha256": .string(validated.snapshotSHA256),
            "resources": .array(validated.resources),
        ])
    }

    static func readV2Resource(
        record: NativeInstalledPluginRecord,
        componentKey: String,
        expectedSnapshot: NativeJSONValue,
        relativePath: String,
        offset: Int,
        maximumCharacters: Int,
        fileManager: FileManager = .default
    ) throws -> NativeJSONValue {
        let validated = try validatedV2Snapshot(
            record: record,
            componentKey: componentKey,
            expectedSnapshot: expectedSnapshot,
            fileManager: fileManager
        )
        let normalizedPath = try normalizedRelativePath(relativePath)
        guard normalizedPath != "SKILL.md",
              let descriptor = validated.resources.first(where: {
                  $0.jsonObject?["relative_path"]?.jsonString == normalizedPath
              }),
              let descriptorObject = descriptor.jsonObject else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Skill 资源不在固定快照中")
        }
        let kind = descriptorObject["kind"]?.jsonString ?? "other"
        guard kind == "reference" || kind == "schema" || kind == "other" else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Skill 资源不是可读取的文本资源")
        }
        let resourceURL = validated.collectionURL
            .appendingPathComponent(normalizedPath, isDirectory: false)
            .standardizedFileURL
        let data = try readRegularFile(
            resourceURL,
            beneath: validated.installationURL,
            maximumBytes: maximumResourceBytes,
            fileManager: fileManager
        )
        guard NativePluginHash.sha256(data) == descriptorObject["sha256"]?.jsonString else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 资源哈希与固定快照不匹配")
        }
        guard let text = String(data: data, encoding: .utf8) else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Skill 文本资源不是 UTF-8")
        }
        let characters = Array(text)
        guard offset >= 0, offset <= characters.count else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Skill 资源 offset 无效")
        }
        let limit = min(max(maximumCharacters, 1), 64_000)
        let end = min(offset + limit, characters.count)
        return .object([
            "skill_id": .string(componentKey),
            "relative_path": .string(normalizedPath),
            "sha256": descriptorObject["sha256"] ?? .null,
            "content": .string(String(characters[offset..<end])),
            "offset": .number(Double(offset)),
            "next_offset": end < characters.count ? .number(Double(end)) : .null,
            "truncated": .bool(end < characters.count),
        ])
    }

    private struct ValidatedV2Snapshot {
        var installationURL: URL
        var collectionURL: URL
        var skillData: Data
        var instructionsSHA256: String
        var resourceManifestSHA256: String
        var snapshotSHA256: String
        var resources: [NativeJSONValue]
        var catalogSnapshot: NativeJSONValue
    }

    private static func validatedV2Snapshot(
        record: NativeInstalledPluginRecord,
        componentKey: String,
        expectedSnapshot: NativeJSONValue,
        fileManager: FileManager
    ) throws -> ValidatedV2Snapshot {
        let expected = try expectedSnapshot.requireObject()
        guard expected["protocol_version"]?.jsonNumber == Double(protocolVersion),
              try expected.requireString("skill_id") == componentKey,
              let metadata = expected["metadata"],
              let expectedResources = expected["resources"]?.jsonArray else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Skill v2 固定快照无效")
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
        let relativeSkillPath = relativeCollectionPath + "/SKILL.md"
        guard try expected.requireString("relative_skill_path") == relativeSkillPath else {
            throw NativePluginRuntimeError.invalidRequest("Plugin Skill 路径与固定快照不匹配")
        }
        let collectionURL = installationURL
            .appendingPathComponent(relativeCollectionPath, isDirectory: true)
            .standardizedFileURL
        try validateDirectory(collectionURL, beneath: installationURL, fileManager: fileManager)
        let skillData = try readRegularFile(
            collectionURL.appendingPathComponent("SKILL.md", isDirectory: false),
            beneath: installationURL,
            maximumBytes: maximumInstructionsBytes,
            fileManager: fileManager
        )
        let instructionsSHA256 = NativePluginHash.sha256(skillData)
        guard try expected.requireString("instructions_sha256") == instructionsSHA256 else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 指令哈希与固定快照不匹配")
        }
        let resources = try resourceDescriptorsV2(
            collectionURL: collectionURL,
            installationURL: installationURL,
            fileManager: fileManager
        )
        guard resources == expectedResources else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 资源目录与固定快照不匹配")
        }
        let resourceManifestSHA256 = try NativePluginHash.canonicalSHA256(.array(resources))
        guard try expected.requireString("resource_manifest_sha256") == resourceManifestSHA256 else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 资源摘要与固定快照不匹配")
        }
        let snapshotPayload: NativeJSONValue = .object([
            "protocol_version": .number(Double(protocolVersion)),
            "skill_id": .string(componentKey),
            "relative_skill_path": .string(relativeSkillPath),
            "metadata": metadata,
            "instructions_sha256": .string(instructionsSHA256),
            "resource_manifest_sha256": .string(resourceManifestSHA256),
        ])
        let snapshotSHA256 = try NativePluginHash.canonicalSHA256(snapshotPayload)
        guard try expected.requireString("snapshot_sha256") == snapshotSHA256 else {
            throw NativePluginRuntimeError.invalidManifest("Plugin Skill 内容摘要与固定快照不匹配")
        }
        let catalogSnapshot: NativeJSONValue = .object([
            "protocol_version": .number(Double(protocolVersion)),
            "skill_id": .string(componentKey),
            "relative_skill_path": .string(relativeSkillPath),
            "metadata": metadata,
            "instructions_sha256": .string(instructionsSHA256),
            "resource_manifest_sha256": .string(resourceManifestSHA256),
            "resources": .array(resources),
            "snapshot_sha256": .string(snapshotSHA256),
        ])
        return ValidatedV2Snapshot(
            installationURL: installationURL,
            collectionURL: collectionURL,
            skillData: skillData,
            instructionsSHA256: instructionsSHA256,
            resourceManifestSHA256: resourceManifestSHA256,
            snapshotSHA256: snapshotSHA256,
            resources: resources,
            catalogSnapshot: catalogSnapshot
        )
    }

    private static func resourceDescriptorsV2(
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
                fileURL.standardizedFileURL.path.dropFirst(collectionURL.path.count + 1)
            )
            resources.append(.object([
                "relative_path": .string(relativePath),
                "sha256": .string(NativePluginHash.sha256(data)),
                "size_bytes": .number(Double(data.count)),
                "kind": .string(resourceKindV2(relativePath)),
            ]))
        }
        return resources.sorted {
            ($0.jsonObject?["relative_path"]?.jsonString ?? "")
                < ($1.jsonObject?["relative_path"]?.jsonString ?? "")
        }
    }

    private static func resourceKindV2(_ relativePath: String) -> String {
        switch relativePath.split(separator: "/").first.map(String.init) ?? "" {
        case "references": return "reference"
        case "scripts": return "script"
        case "assets": return "asset"
        case _ where relativePath.hasSuffix(".json") || relativePath.hasSuffix(".schema.json"):
            return "schema"
        default: return "other"
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
