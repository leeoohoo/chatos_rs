import Foundation

struct NativeApprovalAgentTools: Sendable {
    private let maximumReadBytes = 128 * 1_024
    private let maximumSearchFiles = 5_000
    private let maximumSearchBytes = 20 * 1_024 * 1_024

    func execute(name: String, arguments: [String: Any], projectRoot: URL) -> String {
        do {
            switch name {
            case "read_file_raw":
                return try readFile(arguments, projectRoot: projectRoot)
            case "read_file_range":
                return try readRange(arguments, projectRoot: projectRoot)
            case "list_dir":
                return try listDirectory(arguments, projectRoot: projectRoot)
            case "search_text":
                return try searchText(arguments, projectRoot: projectRoot)
            default:
                return "工具不可用：\(name)"
            }
        } catch {
            return "工具执行失败：\(error.localizedDescription)"
        }
    }

    private func readFile(_ arguments: [String: Any], projectRoot: URL) throws -> String {
        let file = try resolve(arguments["path"] as? String, root: projectRoot)
        let data = try boundedData(file)
        return String(data: data, encoding: .utf8) ?? "文件不是 UTF-8 文本"
    }

    private func readRange(_ arguments: [String: Any], projectRoot: URL) throws -> String {
        let file = try resolve(arguments["path"] as? String, root: projectRoot)
        let start = max(1, arguments["start_line"] as? Int ?? 1)
        let end = min(start + 399, max(start, arguments["end_line"] as? Int ?? start))
        let text = String(data: try boundedData(file), encoding: .utf8) ?? ""
        return text.split(separator: "\n", omittingEmptySubsequences: false)
            .enumerated()
            .filter { ($0.offset + 1) >= start && ($0.offset + 1) <= end }
            .map { "\($0.offset + 1): \($0.element)" }
            .joined(separator: "\n")
    }

    private func listDirectory(_ arguments: [String: Any], projectRoot: URL) throws -> String {
        let directory = try resolve(arguments["path"] as? String ?? ".", root: projectRoot)
        let values = try FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: [.isDirectoryKey, .fileSizeKey],
            options: [.skipsHiddenFiles]
        )
        return values.prefix(200).map { item in
            let metadata = try? item.resourceValues(forKeys: [.isDirectoryKey, .fileSizeKey])
            return metadata?.isDirectory == true
                ? "dir  \(item.lastPathComponent)/"
                : "file \(item.lastPathComponent) \(metadata?.fileSize ?? 0)B"
        }.joined(separator: "\n")
    }

    private func searchText(_ arguments: [String: Any], projectRoot: URL) throws -> String {
        guard let query = (arguments["query"] as? String)?.trimmedNonEmpty else {
            throw NativeApprovalToolError.invalidArguments
        }
        let searchRoot = try resolve(arguments["path"] as? String ?? ".", root: projectRoot)
        guard let enumerator = FileManager.default.enumerator(
            at: searchRoot,
            includingPropertiesForKeys: [.isRegularFileKey, .fileSizeKey],
            options: [.skipsHiddenFiles, .skipsPackageDescendants]
        ) else {
            throw NativeApprovalToolError.unreadablePath
        }
        var inspectedFiles = 0
        var inspectedBytes = 0
        var matches: [String] = []
        for case let file as URL in enumerator {
            let values = try? file.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
            guard values?.isRegularFile == true else { continue }
            let size = values?.fileSize ?? 0
            guard size <= maximumReadBytes else { continue }
            inspectedFiles += 1
            inspectedBytes += size
            if inspectedFiles > maximumSearchFiles || inspectedBytes > maximumSearchBytes { break }
            guard let text = try? String(contentsOf: file, encoding: .utf8) else { continue }
            for (index, line) in text.split(separator: "\n", omittingEmptySubsequences: false).enumerated()
                where line.localizedCaseInsensitiveContains(query) {
                matches.append("\(relative(file, to: projectRoot)):\(index + 1): \(line.prefix(500))")
                if matches.count >= 100 { return matches.joined(separator: "\n") }
            }
        }
        return matches.isEmpty ? "未找到匹配内容" : matches.joined(separator: "\n")
    }

    private func resolve(_ rawPath: String?, root: URL) throws -> URL {
        guard let rawPath = rawPath?.trimmedNonEmpty, !rawPath.hasPrefix("/") else {
            throw NativeApprovalToolError.pathOutsideProject
        }
        let canonicalRoot = root.standardizedFileURL.resolvingSymlinksInPath()
        let candidate = canonicalRoot
            .appendingPathComponent(rawPath)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        let rootPath = canonicalRoot.path.hasSuffix("/") ? canonicalRoot.path : canonicalRoot.path + "/"
        guard candidate.path == canonicalRoot.path || candidate.path.hasPrefix(rootPath) else {
            throw NativeApprovalToolError.pathOutsideProject
        }
        return candidate
    }

    private func boundedData(_ file: URL) throws -> Data {
        let values = try file.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        guard values.isRegularFile == true,
              (values.fileSize ?? maximumReadBytes + 1) <= maximumReadBytes else {
            throw NativeApprovalToolError.fileTooLarge
        }
        return try Data(contentsOf: file, options: .mappedIfSafe)
    }

    private func relative(_ file: URL, to root: URL) -> String {
        let rootPath = root.standardizedFileURL.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        let path = file.standardizedFileURL.path
        return path.replacingOccurrences(of: "/\(rootPath)/", with: "")
    }
}

private enum NativeApprovalToolError: LocalizedError {
    case invalidArguments
    case pathOutsideProject
    case unreadablePath
    case fileTooLarge

    var errorDescription: String? {
        switch self {
        case .invalidArguments: "工具参数无效"
        case .pathOutsideProject: "路径超出当前项目范围"
        case .unreadablePath: "无法读取路径"
        case .fileTooLarge: "文件不是普通文件或超过 128 KB"
        }
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
