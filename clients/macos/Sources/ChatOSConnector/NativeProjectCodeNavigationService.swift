import ChatOSCore
import Foundation

public actor NativeProjectCodeNavigationService: ProjectCodeNavigationServicing {
    private let connector: NativeLocalConnectorService
    private var searchCache: [SearchCacheKey: SearchCacheEntry] = [:]

    public init(connector: NativeLocalConnectorService) {
        self.connector = connector
    }

    public func definition(
        projectRoot: String,
        filePath: String,
        line: Int,
        column: Int
    ) async throws -> ProjectCodeNavigationResult {
        let context = try await context(projectRoot: projectRoot, filePath: filePath)
        guard let token = Self.token(atLine: line, column: column, in: context.content) else {
            return context.result(mode: "heuristic", token: nil, locations: [])
        }
        let hits = try await search(context: context, token: token, limit: 240)
        let locations = hits.compactMap { hit -> ProjectCodeNavigationLocation? in
            let score = Self.definitionScore(context: context, hit: hit, token: token, line: line)
            guard score >= 2 else { return nil }
            let location = context.location(for: hit, token: token, score: score)
            return Self.isRequestLocation(location, context: context, line: line, column: column)
                ? nil
                : location
        }
        .sorted {
            if $0.score != $1.score { return $0.score > $1.score }
            if $0.relativePath != $1.relativePath { return $0.relativePath < $1.relativePath }
            return $0.line < $1.line
        }
        return context.result(mode: "heuristic", token: token, locations: Array(locations.prefix(20)))
    }

    public func references(
        projectRoot: String,
        filePath: String,
        line: Int,
        column: Int
    ) async throws -> ProjectCodeNavigationResult {
        let context = try await context(projectRoot: projectRoot, filePath: filePath)
        guard let token = Self.token(atLine: line, column: column, in: context.content) else {
            return context.result(mode: "text-search", token: nil, locations: [])
        }
        let hits = try await search(context: context, token: token, limit: 500)
        let locations = hits.compactMap { hit -> ProjectCodeNavigationLocation? in
            guard Self.tokenColumn(in: hit.text, token: token) != nil else { return nil }
            let score = hit.relativePath == context.fileProjectRelative ? 1.5 : 1
            let location = context.location(for: hit, token: token, score: score)
            return Self.isRequestLocation(location, context: context, line: line, column: column)
                ? nil
                : location
        }
        .sorted {
            if $0.relativePath != $1.relativePath { return $0.relativePath < $1.relativePath }
            if $0.line != $1.line { return $0.line < $1.line }
            return $0.column < $1.column
        }
        return context.result(mode: "text-search", token: token, locations: Array(locations.prefix(200)))
    }

    private func context(projectRoot: String, filePath: String) async throws -> Context {
        let root = try await connector.resolveProjectPath(projectRoot)
        let file = try await connector.resolveProjectPath(filePath)
        guard root.workspace.id == file.workspace.id,
              Self.isInside(file.absoluteURL, root: root.absoluteURL),
              file.absoluteURL.path != root.absoluteURL.path else {
            throw NavigationError.fileOutsideProject
        }
        let readTask = Task.detached {
            try String(contentsOf: file.absoluteURL, encoding: .utf8)
        }
        let content = try await withTaskCancellationHandler {
            try await readTask.value
        } onCancel: {
            readTask.cancel()
        }
        let fileProjectRelative = Self.projectRelativePath(
            workspaceRelativePath: file.relativePath,
            rootRelativePath: root.relativePath
        )
        return Context(
            root: root,
            file: file,
            fileProjectRelative: fileProjectRelative,
            content: content,
            language: Self.language(for: file.absoluteURL)
        )
    }

    private func search(context: Context, token: String, limit: Int) async throws -> [SearchHit] {
        let cacheKey = SearchCacheKey(
            workspaceID: context.root.workspace.id,
            rootPath: context.root.relativePath,
            token: token
        )
        if let cached = searchCache[cacheKey], Date().timeIntervalSince(cached.createdAt) < 8 {
            return Array(cached.hits.prefix(limit))
        }

        let searchTask = Task.detached {
            try NativeWorkspaceFilesystem(workspace: context.root.workspace)
                .searchContent(path: context.root.relativePath, query: token, limit: 500)
        }
        let value = try await withTaskCancellationHandler {
            try await searchTask.value
        } onCancel: {
            searchTask.cancel()
        }
        guard case let .object(object) = value,
              case let .array(items)? = object["matches"] else { return [] }
        let hits: [SearchHit] = items.compactMap { item -> SearchHit? in
            guard case let .object(fields) = item,
                  case let .string(path)? = fields["path"],
                  case let .number(line)? = fields["line"],
                  case let .string(text)? = fields["text"],
                  let relative = Self.projectRelativePathIfContained(
                    workspaceRelativePath: path,
                    rootRelativePath: context.root.relativePath
                  ) else { return nil }
            return SearchHit(workspaceRelativePath: path, relativePath: relative, line: Int(line), text: text)
        }
        guard !Task.isCancelled else { throw CancellationError() }
        searchCache[cacheKey] = SearchCacheEntry(createdAt: Date(), hits: hits)
        if searchCache.count > 24 {
            let oldestKeys = searchCache
                .sorted { $0.value.createdAt < $1.value.createdAt }
                .prefix(searchCache.count - 24)
                .map(\.key)
            for key in oldestKeys { searchCache.removeValue(forKey: key) }
        }
        return Array(hits.prefix(limit))
    }
}

private extension NativeProjectCodeNavigationService {
    struct Context: Sendable {
        let root: NativeResolvedProjectPath
        let file: NativeResolvedProjectPath
        let fileProjectRelative: String
        let content: String
        let language: String

        func location(for hit: SearchHit, token: String, score: Double) -> ProjectCodeNavigationLocation {
            let column = NativeProjectCodeNavigationService.tokenColumn(in: hit.text, token: token) ?? 1
            return .init(
                path: root.logicalPath(for: hit.workspaceRelativePath),
                relativePath: hit.relativePath,
                line: hit.line,
                column: column,
                endLine: hit.line,
                endColumn: column + max(0, token.count - 1),
                preview: hit.text,
                score: score
            )
        }

        func result(
            mode: String,
            token: String?,
            locations: [ProjectCodeNavigationLocation]
        ) -> ProjectCodeNavigationResult {
            .init(
                provider: "native-local-fallback",
                language: language,
                mode: mode,
                token: token,
                locations: locations
            )
        }
    }

    struct SearchHit: Sendable {
        let workspaceRelativePath: String
        let relativePath: String
        let line: Int
        let text: String
    }

    struct SearchCacheKey: Hashable {
        let workspaceID: String
        let rootPath: String
        let token: String
    }

    struct SearchCacheEntry {
        let createdAt: Date
        let hits: [SearchHit]
    }

    enum NavigationError: LocalizedError {
        case fileOutsideProject

        var errorDescription: String? { "当前文件不在项目目录内，无法执行代码导航。" }
    }

    static func token(atLine line: Int, column: Int, in content: String) -> String? {
        let lines = content.split(separator: "\n", omittingEmptySubsequences: false)
        guard line > 0, line <= lines.count else { return nil }
        let characters = Array(lines[line - 1])
        guard !characters.isEmpty else { return nil }
        var index = min(max(column - 1, 0), characters.count - 1)
        if !isTokenCharacter(characters[index]), index > 0, isTokenCharacter(characters[index - 1]) {
            index -= 1
        }
        guard isTokenCharacter(characters[index]) else { return nil }
        var start = index
        var end = index
        while start > 0, isTokenCharacter(characters[start - 1]) { start -= 1 }
        while end + 1 < characters.count, isTokenCharacter(characters[end + 1]) { end += 1 }
        return String(characters[start...end])
    }

    static func tokenColumn(in line: String, token: String) -> Int? {
        guard !token.isEmpty else { return nil }
        let characters = Array(line)
        let needle = Array(token)
        guard characters.count >= needle.count else { return nil }
        for start in 0...(characters.count - needle.count) {
            let end = start + needle.count
            guard Array(characters[start..<end]) == needle else { continue }
            let beforeIsBoundary = start == 0 || !isTokenCharacter(characters[start - 1])
            let afterIsBoundary = end == characters.count || !isTokenCharacter(characters[end])
            if beforeIsBoundary && afterIsBoundary { return start + 1 }
        }
        return nil
    }

    static func definitionScore(context: Context, hit: SearchHit, token: String, line: Int) -> Double {
        let lower = hit.text.lowercased()
        let tokenLower = token.lowercased()
        let fileStem = URL(fileURLWithPath: hit.relativePath).deletingPathExtension().lastPathComponent
        var score = 0.0
        if hit.relativePath == context.fileProjectRelative { score += 1.5 }
        if fileStem.caseInsensitiveCompare(token) == .orderedSame { score += 4 }
        if hit.relativePath == context.fileProjectRelative, hit.line == line { score -= 3 }
        let patterns = [
            "class \(tokenLower)", "interface \(tokenLower)", "enum \(tokenLower)",
            "struct \(tokenLower)", "type \(tokenLower) ", "func \(tokenLower)",
            "fn \(tokenLower)", "def \(tokenLower)", "function \(tokenLower)",
            "const \(tokenLower) =", "let \(tokenLower) =", "var \(tokenLower) =",
            "const \(tokenLower):", "let \(tokenLower):", "var \(tokenLower):",
        ]
        score += Double(patterns.filter(lower.contains).count) * 2
        if lower.trimmingCharacters(in: .whitespaces).hasPrefix(tokenLower) { score += 1 }
        return score
    }

    static func isRequestLocation(
        _ location: ProjectCodeNavigationLocation,
        context: Context,
        line: Int,
        column: Int
    ) -> Bool {
        location.relativePath == context.fileProjectRelative
            && location.line == line
            && location.column <= column
            && location.endColumn >= column
    }

    static func isTokenCharacter(_ character: Character) -> Bool {
        character.isLetter || character.isNumber || character == "_" || character == "$"
    }

    static func isInside(_ file: URL, root: URL) -> Bool {
        let filePath = file.standardizedFileURL.resolvingSymlinksInPath().path
        let rootPath = root.standardizedFileURL.resolvingSymlinksInPath().path
        return filePath == rootPath || filePath.hasPrefix(rootPath.hasSuffix("/") ? rootPath : rootPath + "/")
    }

    static func projectRelativePath(workspaceRelativePath: String, rootRelativePath: String) -> String {
        projectRelativePathIfContained(
            workspaceRelativePath: workspaceRelativePath,
            rootRelativePath: rootRelativePath
        ) ?? workspaceRelativePath
    }

    static func projectRelativePathIfContained(
        workspaceRelativePath: String,
        rootRelativePath: String
    ) -> String? {
        if rootRelativePath == "." || rootRelativePath.isEmpty { return workspaceRelativePath }
        if workspaceRelativePath == rootRelativePath { return "" }
        let prefix = rootRelativePath + "/"
        guard workspaceRelativePath.hasPrefix(prefix) else { return nil }
        return String(workspaceRelativePath.dropFirst(prefix.count))
    }

    static func language(for file: URL) -> String {
        let name = file.lastPathComponent.lowercased()
        if name == "dockerfile" { return "dockerfile" }
        switch file.pathExtension.lowercased() {
        case "java": return "java"
        case "ts", "tsx", "mts", "cts": return "typescript"
        case "js", "jsx", "mjs", "cjs": return "javascript"
        case "rs": return "rust"
        case "go": return "go"
        case "py": return "python"
        case "kt", "kts": return "kotlin"
        case "swift": return "swift"
        case "php": return "php"
        case "rb": return "ruby"
        case "cs": return "csharp"
        case "cpp", "cc", "cxx", "hpp", "hh", "h", "hxx", "ipp": return "cpp"
        case "c": return "c"
        case "sh", "bash", "zsh": return "shell"
        case "json": return "json"
        case "yaml", "yml": return "yaml"
        case "toml": return "toml"
        case "md", "markdown": return "markdown"
        default: return "unknown"
        }
    }
}
