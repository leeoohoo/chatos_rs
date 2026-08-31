import CryptoKit
import Foundation

struct NativeMCPCodeWriteScope: Hashable, Sendable {
    let workspaceID: String
    let sessionID: String
    let runID: String
}

actor NativeMCPCodeWriteStore {
    private static let maximumWriteBytes = 2 * 1_024 * 1_024

    private var sessions: [String: EditSession] = [:]
    private var sessionByScope: [NativeMCPCodeWriteScope: String] = [:]

    static var toolNames: Set<String> {
        ["open_edit_session", "stage_edit_batch", "commit_edit_session", "abort_edit_session"]
    }

    static var toolDefinitions: [NativeJSONValue] {
        [
            definition(
                name: "open_edit_session",
                description: "打开文件编辑会话。修改会先保存在内存中，提交前不会写入项目。",
                properties: [
                    "purpose": .object(["type": .string("string")]),
                    "fresh": .object(["type": .string("boolean")]),
                ],
                required: []
            ),
            definition(
                name: "stage_edit_batch",
                description: "向编辑会话暂存一组有序的文件修改。",
                properties: [
                    "session_id": .object(["type": .string("string"), "minLength": .number(1)]),
                    "operations": .object([
                        "type": .string("array"),
                        "minItems": .number(1),
                        "items": .object([
                            "type": .string("object"),
                            "properties": .object([
                                "kind": .object([
                                    "type": .string("string"),
                                    "enum": .array(["write", "replace_text", "append", "delete"].map(NativeJSONValue.string)),
                                ]),
                                "path": .object(["type": .string("string")]),
                                "content": .object(["type": .string("string")]),
                                "old_text": .object(["type": .string("string")]),
                                "new_text": .object(["type": .string("string")]),
                                "start_line": .object(["type": .string("integer"), "minimum": .number(1)]),
                                "end_line": .object(["type": .string("integer"), "minimum": .number(1)]),
                                "before_context": .object(["type": .string("string")]),
                                "after_context": .object(["type": .string("string")]),
                                "expected_matches": .object(["type": .string("integer"), "minimum": .number(1)]),
                                "expected_sha256": .object([
                                    "type": .array([.string("string"), .string("null")]),
                                    "pattern": .string("^[0-9a-f]{64}$"),
                                ]),
                            ]),
                            "required": .array(["kind", "path"].map(NativeJSONValue.string)),
                            "additionalProperties": .bool(false),
                        ]),
                    ]),
                ],
                required: ["session_id", "operations"]
            ),
            definition(
                name: "commit_edit_session",
                description: "校验所有文件仍与读取时一致，然后提交整批暂存修改。",
                properties: ["session_id": .object(["type": .string("string"), "minLength": .number(1)])],
                required: ["session_id"]
            ),
            definition(
                name: "abort_edit_session",
                description: "放弃编辑会话中的全部暂存修改。",
                properties: ["session_id": .object(["type": .string("string"), "minLength": .number(1)])],
                required: ["session_id"]
            ),
        ]
    }

    func call(
        name: String,
        arguments: [String: NativeJSONValue],
        scope: NativeMCPCodeWriteScope,
        projectRoot: URL
    ) throws -> NativeJSONValue {
        switch name {
        case "open_edit_session":
            return open(arguments: arguments, scope: scope, projectRoot: projectRoot)
        case "stage_edit_batch":
            return try stage(arguments: arguments, scope: scope)
        case "commit_edit_session":
            return try commit(arguments: arguments, scope: scope)
        case "abort_edit_session":
            return try abort(arguments: arguments, scope: scope)
        default:
            throw NativeMCPCodeWriteError.unsupportedTool(name)
        }
    }

    private func open(
        arguments: [String: NativeJSONValue],
        scope: NativeMCPCodeWriteScope,
        projectRoot: URL
    ) -> NativeJSONValue {
        let fresh = arguments.bool("fresh") ?? false
        if !fresh,
           let existingID = sessionByScope[scope],
           let existing = sessions[existingID] {
            return sessionResult(existing, reused: true)
        }
        if let existingID = sessionByScope.removeValue(forKey: scope) {
            sessions.removeValue(forKey: existingID)
        }
        let session = EditSession(
            id: UUID().uuidString,
            scope: scope,
            projectRoot: projectRoot.standardizedFileURL.resolvingSymlinksInPath(),
            purpose: arguments.string("purpose"),
            files: [:],
            stagedOperationCount: 0,
            openedAt: Date()
        )
        sessions[session.id] = session
        sessionByScope[scope] = session.id
        return sessionResult(session, reused: false)
    }

    private func stage(
        arguments: [String: NativeJSONValue],
        scope: NativeMCPCodeWriteScope
    ) throws -> NativeJSONValue {
        let sessionID = try requiredString(arguments, "session_id")
        guard var session = sessions[sessionID], session.scope == scope else {
            throw NativeMCPCodeWriteError.sessionUnavailable
        }
        guard case let .array(operationValues)? = arguments["operations"], !operationValues.isEmpty else {
            throw NativeMCPCodeWriteError.invalidArguments("operations 至少需要一项")
        }

        var changedPaths = Set<String>()
        var matches: [NativeJSONValue] = []
        for operationValue in operationValues {
            guard case let .object(operation) = operationValue else {
                throw NativeMCPCodeWriteError.invalidArguments("operations 中存在无效项")
            }
            let result = try apply(operation: operation, session: &session)
            if result.changed { changedPaths.insert(result.path) }
            if let match = result.match {
                matches.append(.object(["path": .string(result.path), "match": match]))
            }
        }
        session.stagedOperationCount += operationValues.count
        sessions[sessionID] = session

        let pending = session.files.values.filter(\.hasChanges).sorted { $0.path < $1.path }
        return .object([
            "outcome": .string(changedPaths.isEmpty ? "already_applied" : "applied"),
            "changed": .bool(!changedPaths.isEmpty),
            "changed_target_count": .number(Double(changedPaths.count)),
            "result": .object([
                "session_id": .string(sessionID),
                "staged_operation_count": .number(Double(session.stagedOperationCount)),
                "batch_operation_count": .number(Double(operationValues.count)),
                "batch_changed_paths": .array(changedPaths.sorted().map(NativeJSONValue.string)),
                "pending_target_count": .number(Double(pending.count)),
                "pending_paths": .array(pending.map(pathSummary)),
            ]),
            "matches": .array(matches),
        ])
    }

    private func commit(
        arguments: [String: NativeJSONValue],
        scope: NativeMCPCodeWriteScope
    ) throws -> NativeJSONValue {
        let sessionID = try requiredString(arguments, "session_id")
        guard let session = sessions[sessionID], session.scope == scope else {
            throw NativeMCPCodeWriteError.sessionUnavailable
        }
        let changed = session.files.values.filter(\.hasChanges).sorted { $0.path < $1.path }
        try validateCurrentBaselines(changed, root: session.projectRoot)
        removeSession(session)

        guard !changed.isEmpty else {
            return .object([
                "outcome": .string("already_applied"),
                "changed": .bool(false),
                "changed_target_count": .number(0),
                "result": .object([
                    "session_id": .string(sessionID),
                    "committed_paths": .array([]),
                    "staged_operation_count": .number(Double(session.stagedOperationCount)),
                ]),
            ])
        }

        do {
            try apply(states: changed, root: session.projectRoot)
        } catch {
            throw NativeMCPCodeWriteError.commitFailed(error.localizedDescription)
        }
        return .object([
            "outcome": .string("applied"),
            "changed": .bool(true),
            "changed_target_count": .number(Double(changed.count)),
            "result": .object([
                "session_id": .string(sessionID),
                "committed_paths": .array(changed.map { .string($0.path) }),
                "staged_operation_count": .number(Double(session.stagedOperationCount)),
            ]),
            "files": .array(changed.map { state in
                .object([
                    "path": .string(state.path),
                    "change_kind": .string(state.working.kind == .missing ? "delete" : state.base.kind == .missing ? "create" : "update"),
                    "deleted": .bool(state.working.kind == .missing),
                ])
            }),
        ])
    }

    private func abort(
        arguments: [String: NativeJSONValue],
        scope: NativeMCPCodeWriteScope
    ) throws -> NativeJSONValue {
        let sessionID = try requiredString(arguments, "session_id")
        guard let session = sessions[sessionID], session.scope == scope else {
            throw NativeMCPCodeWriteError.sessionUnavailable
        }
        removeSession(session)
        return .object([
            "outcome": .string("already_applied"),
            "changed": .bool(false),
            "changed_target_count": .number(0),
            "result": .object([
                "session_id": .string(sessionID),
                "discarded_target_count": .number(Double(session.files.values.filter(\.hasChanges).count)),
                "staged_operation_count": .number(Double(session.stagedOperationCount)),
            ]),
        ])
    }

    private func apply(
        operation: [String: NativeJSONValue],
        session: inout EditSession
    ) throws -> StageResult {
        let kind = try requiredString(operation, "kind")
        let path = try normalizePath(requiredString(operation, "path"))
        try validateNoPathOverlap(path, kind: kind, files: session.files)
        var state = try stateForOperation(path: path, operation: operation, session: session)
        var match: NativeJSONValue?

        switch kind {
        case "write":
            let content = try requiredRawString(operation, "content")
            try validateWriteSize(content)
            guard state.working.kind != .directory else { throw NativeMCPCodeWriteError.targetIsDirectory }
            state.working = .file(content)
        case "append":
            let content = try requiredRawString(operation, "content")
            guard state.working.kind != .directory else { throw NativeMCPCodeWriteError.targetIsDirectory }
            let next = (state.working.content ?? "") + content
            try validateWriteSize(next)
            state.working = .file(next)
        case "replace_text":
            guard state.working.kind == .file else { throw NativeMCPCodeWriteError.targetIsNotFile }
            let oldText = try requiredRawString(operation, "old_text")
            let newText = try requiredRawString(operation, "new_text")
            let replacement = try replace(
                oldText: oldText,
                newText: newText,
                in: state.working.content ?? "",
                startLine: operation.integer("start_line"),
                endLine: operation.integer("end_line"),
                expectedMatches: operation.integer("expected_matches")
            )
            try validateWriteSize(replacement.content)
            state.working = .file(replacement.content)
            match = .object([
                "match_count": .number(Double(replacement.matchCount)),
                "changed": .bool(replacement.changed),
            ])
        case "delete":
            state.working = .missing
        default:
            throw NativeMCPCodeWriteError.invalidArguments("不支持的修改类型：\(kind)")
        }

        state.stagedOperations += 1
        let changed = state.hasChanges
        session.files[path] = state
        return StageResult(path: path, changed: changed, match: match)
    }

    private func stateForOperation(
        path: String,
        operation: [String: NativeJSONValue],
        session: EditSession
    ) throws -> FileState {
        if let state = session.files[path] {
            if let expected = expectedSHA(operation),
               !expected.matches(state.base), !expected.matches(state.working) {
                throw NativeMCPCodeWriteError.staleContext(path)
            }
            return state
        }
        let snapshot = try snapshot(path: path, root: session.projectRoot)
        let isNoOpWrite: Bool
        if operation.string("kind") == "write", case let .string(content)? = operation["content"] {
            isNoOpWrite = snapshot.kind == .file && snapshot.content == content
        } else {
            isNoOpWrite = false
        }
        if !isNoOpWrite {
            guard operation.keys.contains("expected_sha256") else {
                throw NativeMCPCodeWriteError.invalidArguments("首次修改 \(path) 时必须提供 expected_sha256")
            }
            guard let expected = expectedSHA(operation), expected.matches(snapshot) else {
                throw NativeMCPCodeWriteError.staleContext(path)
            }
        }
        return FileState(path: path, base: snapshot, working: snapshot, stagedOperations: 0)
    }

    private func expectedSHA(_ operation: [String: NativeJSONValue]) -> ExpectedSHA? {
        guard let value = operation["expected_sha256"] else { return nil }
        switch value {
        case .null: return .missing
        case let .string(hash): return .hash(hash)
        default: return nil
        }
    }

    private func snapshot(path: String, root: URL) throws -> FileSnapshot {
        let url = try resolve(path: path, root: root, permitsMissingLeaf: true)
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) else {
            return .missing
        }
        let values = try url.resourceValues(forKeys: [.isSymbolicLinkKey, .isRegularFileKey, .isDirectoryKey, .fileSizeKey])
        guard values.isSymbolicLink != true else { throw NativeMCPCodeWriteError.symbolicLink }
        if isDirectory.boolValue { return .directory }
        guard values.isRegularFile == true else { throw NativeMCPCodeWriteError.targetIsNotFile }
        let size = values.fileSize ?? Self.maximumWriteBytes + 1
        guard size <= Self.maximumWriteBytes else { throw NativeMCPCodeWriteError.fileTooLarge }
        let data = try Data(contentsOf: url, options: [.mappedIfSafe])
        guard !data.prefix(8_000).contains(0), let content = String(data: data, encoding: .utf8) else {
            throw NativeMCPCodeWriteError.binaryFile
        }
        return .file(content)
    }

    private func validateCurrentBaselines(_ states: [FileState], root: URL) throws {
        let conflicts = try states.compactMap { state -> String? in
            let current = try snapshot(path: state.path, root: root)
            return current == state.base ? nil : state.path
        }
        guard conflicts.isEmpty else {
            throw NativeMCPCodeWriteError.commitConflict(conflicts)
        }
    }

    private func apply(states: [FileState], root: URL) throws {
        var applied: [FileState] = []
        do {
            for state in states {
                try apply(snapshot: state.working, path: state.path, root: root)
                applied.append(state)
            }
        } catch {
            for state in applied.reversed() {
                try? apply(snapshot: state.base, path: state.path, root: root)
            }
            throw error
        }
    }

    private func apply(snapshot: FileSnapshot, path: String, root: URL) throws {
        let url = try resolve(path: path, root: root, permitsMissingLeaf: true)
        switch snapshot.kind {
        case .missing:
            if FileManager.default.fileExists(atPath: url.path) {
                try FileManager.default.removeItem(at: url)
            }
        case .directory:
            try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        case .file:
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try Data((snapshot.content ?? "").utf8).write(to: url, options: .atomic)
        }
    }

    private func resolve(path: String, root: URL, permitsMissingLeaf: Bool) throws -> URL {
        let normalized = try normalizePath(path)
        let canonicalRoot = root.standardizedFileURL.resolvingSymlinksInPath()
        let candidate = canonicalRoot.appendingPathComponent(normalized).standardizedFileURL
        let parent = candidate.deletingLastPathComponent()
        let canonicalParent = parent.resolvingSymlinksInPath().standardizedFileURL
        guard contains(canonicalParent, root: canonicalRoot) else {
            throw NativeMCPCodeWriteError.pathOutsideProject
        }
        if FileManager.default.fileExists(atPath: candidate.path) {
            let canonical = candidate.resolvingSymlinksInPath().standardizedFileURL
            guard canonical == candidate, contains(canonical, root: canonicalRoot) else {
                throw NativeMCPCodeWriteError.symbolicLink
            }
        } else if !permitsMissingLeaf {
            throw NativeMCPCodeWriteError.targetMissing
        }
        return candidate
    }

    private func normalizePath(_ raw: String) throws -> String {
        let value = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "\\", with: "/")
        guard !value.isEmpty, !value.hasPrefix("/"), !value.contains("\u{0}") else {
            throw NativeMCPCodeWriteError.pathOutsideProject
        }
        let components = value.split(separator: "/", omittingEmptySubsequences: true)
        guard !components.isEmpty, components.allSatisfy({ $0 != "." && $0 != ".." }) else {
            throw NativeMCPCodeWriteError.pathOutsideProject
        }
        return components.joined(separator: "/")
    }

    private func contains(_ url: URL, root: URL) -> Bool {
        url.path == root.path || url.path.hasPrefix(root.path.hasSuffix("/") ? root.path : root.path + "/")
    }

    private func validateNoPathOverlap(
        _ path: String,
        kind: String,
        files: [String: FileState]
    ) throws {
        for existing in files.keys where existing != path {
            if path.hasPrefix(existing + "/") || existing.hasPrefix(path + "/") {
                if kind == "delete" || files[existing]?.working.kind == .missing {
                    throw NativeMCPCodeWriteError.invalidArguments("同一编辑会话不能同时修改互相包含的路径")
                }
            }
        }
    }

    private func replace(
        oldText: String,
        newText: String,
        in content: String,
        startLine: Int?,
        endLine: Int?,
        expectedMatches: Int?
    ) throws -> (content: String, matchCount: Int, changed: Bool) {
        guard !oldText.isEmpty else {
            throw NativeMCPCodeWriteError.invalidArguments("old_text 不能为空")
        }
        let searchRange = lineRange(in: content, startLine: startLine, endLine: endLine)
        var ranges: [Range<String.Index>] = []
        var cursor = searchRange.lowerBound
        while cursor < searchRange.upperBound,
              let range = content.range(of: oldText, range: cursor..<searchRange.upperBound) {
            ranges.append(range)
            cursor = range.upperBound
        }
        let requiredCount = expectedMatches ?? 1
        guard ranges.count == requiredCount else {
            throw NativeMCPCodeWriteError.expectedMatches(expected: requiredCount, actual: ranges.count)
        }
        var result = content
        for range in ranges.reversed() { result.replaceSubrange(range, with: newText) }
        return (result, ranges.count, result != content)
    }

    private func lineRange(in content: String, startLine: Int?, endLine: Int?) -> Range<String.Index> {
        guard startLine != nil || endLine != nil else { return content.startIndex..<content.endIndex }
        let starts = [content.startIndex] + content.indices.filter { content[$0] == "\n" }.map { content.index(after: $0) }
        let lowerLine = max(1, startLine ?? 1)
        let upperLine = max(lowerLine, endLine ?? starts.count)
        let lower = lowerLine <= starts.count ? starts[lowerLine - 1] : content.endIndex
        let upper = upperLine < starts.count ? starts[upperLine] : content.endIndex
        return lower..<upper
    }

    private func validateWriteSize(_ content: String) throws {
        guard content.utf8.count <= Self.maximumWriteBytes else {
            throw NativeMCPCodeWriteError.fileTooLarge
        }
    }

    private func removeSession(_ session: EditSession) {
        sessions.removeValue(forKey: session.id)
        if sessionByScope[session.scope] == session.id {
            sessionByScope.removeValue(forKey: session.scope)
        }
    }

    private func sessionResult(_ session: EditSession, reused: Bool) -> NativeJSONValue {
        .object([
            "outcome": .string("already_applied"),
            "changed": .bool(false),
            "changed_target_count": .number(0),
            "result": .object([
                "session_id": .string(session.id),
                "reused": .bool(reused),
                "purpose": session.purpose.map(NativeJSONValue.string) ?? .null,
            ]),
        ])
    }

    private func pathSummary(_ state: FileState) -> NativeJSONValue {
        .object([
            "path": .string(state.path),
            "change_kind": .string(state.working.kind == .missing ? "delete" : state.base.kind == .missing ? "create" : "update"),
            "staged_operation_count": .number(Double(state.stagedOperations)),
        ])
    }

    private func requiredString(_ values: [String: NativeJSONValue], _ key: String) throws -> String {
        guard let value = values.string(key)?.trimmingCharacters(in: .whitespacesAndNewlines), !value.isEmpty else {
            throw NativeMCPCodeWriteError.invalidArguments("缺少参数：\(key)")
        }
        return value
    }

    private func requiredRawString(_ values: [String: NativeJSONValue], _ key: String) throws -> String {
        guard case let .string(value)? = values[key] else {
            throw NativeMCPCodeWriteError.invalidArguments("缺少参数：\(key)")
        }
        return value
    }

    private static func definition(
        name: String,
        description: String,
        properties: [String: NativeJSONValue],
        required: [String]
    ) -> NativeJSONValue {
        .object([
            "name": .string(name),
            "description": .string(description),
            "inputSchema": .object([
                "type": .string("object"),
                "properties": .object(properties),
                "required": .array(required.map(NativeJSONValue.string)),
                "additionalProperties": .bool(false),
            ]),
        ])
    }
}

private struct EditSession: Sendable {
    let id: String
    let scope: NativeMCPCodeWriteScope
    let projectRoot: URL
    let purpose: String?
    var files: [String: FileState]
    var stagedOperationCount: Int
    let openedAt: Date
}

private struct FileState: Sendable {
    let path: String
    let base: FileSnapshot
    var working: FileSnapshot
    var stagedOperations: Int

    var hasChanges: Bool { base != working }
}

private struct FileSnapshot: Equatable, Sendable {
    enum Kind: String, Sendable { case missing, file, directory }

    let kind: Kind
    let content: String?
    let sha256: String?

    static let missing = FileSnapshot(kind: .missing, content: nil, sha256: nil)
    static let directory = FileSnapshot(kind: .directory, content: nil, sha256: nil)

    static func file(_ content: String) -> FileSnapshot {
        let digest = SHA256.hash(data: Data(content.utf8)).map { String(format: "%02x", $0) }.joined()
        return FileSnapshot(kind: .file, content: content, sha256: digest)
    }
}

private enum ExpectedSHA {
    case missing
    case hash(String)

    func matches(_ snapshot: FileSnapshot) -> Bool {
        switch self {
        case .missing: snapshot.kind != .file
        case let .hash(value): snapshot.kind == .file && snapshot.sha256 == value
        }
    }
}

private struct StageResult {
    let path: String
    let changed: Bool
    let match: NativeJSONValue?
}

enum NativeMCPCodeWriteError: LocalizedError {
    case unsupportedTool(String)
    case invalidArguments(String)
    case sessionUnavailable
    case pathOutsideProject
    case symbolicLink
    case targetMissing
    case targetIsDirectory
    case targetIsNotFile
    case fileTooLarge
    case binaryFile
    case staleContext(String)
    case expectedMatches(expected: Int, actual: Int)
    case commitConflict([String])
    case commitFailed(String)

    var errorDescription: String? {
        switch self {
        case let .unsupportedTool(name): "不支持的代码修改工具：\(name)"
        case let .invalidArguments(message): message
        case .sessionUnavailable: "编辑会话不存在、已结束或不属于当前任务"
        case .pathOutsideProject: "路径超出当前项目范围"
        case .symbolicLink: "不允许通过符号链接修改项目外文件"
        case .targetMissing: "目标不存在"
        case .targetIsDirectory: "目标是目录，不能写入文本"
        case .targetIsNotFile: "目标不是可编辑文本文件"
        case .fileTooLarge: "文件超过本机代码修改工具的大小限制"
        case .binaryFile: "二进制文件不能使用文本修改工具编辑"
        case let .staleContext(path): "文件已发生变化，请重新读取后再修改：\(path)"
        case let .expectedMatches(expected, actual): "文本匹配数量不符合预期：期望 \(expected)，实际 \(actual)"
        case let .commitConflict(paths): "提交前发现文件已变化：\(paths.joined(separator: ", "))"
        case let .commitFailed(message): "提交文件修改失败：\(message)"
        }
    }
}

private extension Dictionary where Key == String, Value == NativeJSONValue {
    func string(_ key: String) -> String? {
        guard case let .string(value)? = self[key] else { return nil }
        return value
    }

    func bool(_ key: String) -> Bool? {
        guard case let .bool(value)? = self[key] else { return nil }
        return value
    }

    func integer(_ key: String) -> Int? {
        guard case let .number(value)? = self[key] else { return nil }
        return Int(value)
    }
}
