import ChatOSCore
import Foundation

enum NativeProjectGitParser {
    static func parseChanges(_ data: Data, repositoryRoot: URL) -> [ProjectGitChange] {
        let tokens = data.split(separator: 0, omittingEmptySubsequences: true)
        var changes: [ProjectGitChange] = []
        var index = 0
        while index < tokens.count {
            let token = tokens[index]
            guard token.count >= 3 else {
                index += 1
                continue
            }
            let indexStatus = String(decoding: token.prefix(1), as: UTF8.self)
            let workTreeStatus = String(decoding: token.dropFirst().prefix(1), as: UTF8.self)
            let path = String(decoding: token.dropFirst(3), as: UTF8.self)
            var originalPath: String?
            if indexStatus == "R" || indexStatus == "C" {
                index += 1
                if index < tokens.count {
                    originalPath = String(decoding: tokens[index], as: UTF8.self)
                }
            }
            changes.append(ProjectGitChange(
                path: path,
                originalPath: originalPath,
                absolutePath: repositoryRoot.appendingPathComponent(path).standardizedFileURL.path,
                indexStatus: indexStatus,
                workTreeStatus: workTreeStatus,
                kind: changeKind(index: indexStatus, workTree: workTreeStatus)
            ))
            index += 1
        }
        return changes.sorted {
            if $0.kind == .conflicted, $1.kind != .conflicted { return true }
            if $0.kind != .conflicted, $1.kind == .conflicted { return false }
            return $0.path.localizedStandardCompare($1.path) == .orderedAscending
        }
    }

    static func parseBranches(_ value: String) -> [ProjectGitBranch] {
        var branches: [ProjectGitBranch] = []
        for line in value.split(whereSeparator: { $0.isNewline }) {
            let fields = line
                .split(separator: "\0", omittingEmptySubsequences: false)
                .map(String.init)
            guard let name = fields.first, !name.isEmpty else { continue }
            let upstream = fields.count > 2
                ? fields[2].trimmingCharacters(in: .whitespacesAndNewlines)
                : ""
            branches.append(ProjectGitBranch(
                name: name,
                isCurrent: fields.count > 1 && fields[1] == "*",
                upstream: upstream.isEmpty ? nil : upstream
            ))
        }
        return branches
    }

    static func parseCommits(_ data: Data) -> [ProjectGitCommit] {
        let formatter = ISO8601DateFormatter()
        return data.split(separator: 0x1e, omittingEmptySubsequences: true).compactMap { record in
            var clean = record
            while clean.first == 0x0a || clean.first == 0x0d { clean = clean.dropFirst() }
            let fields = clean.split(separator: 0, omittingEmptySubsequences: false).map {
                String(decoding: $0, as: UTF8.self)
            }
            guard fields.count >= 7 else { return nil }
            return ProjectGitCommit(
                id: fields[0],
                shortID: fields[1],
                parentIDs: fields[2].split(separator: " ").map(String.init),
                author: fields[3],
                authoredAt: formatter.date(from: fields[4]),
                decorations: fields[5].split(separator: ",").map {
                    $0.trimmingCharacters(in: .whitespaces)
                }.filter { !$0.isEmpty },
                subject: fields[6]
            )
        }
    }

    private static func changeKind(index: String, workTree: String) -> ProjectGitChangeKind {
        let pair = index + workTree
        if ["DD", "AU", "UD", "UA", "DU", "AA", "UU"].contains(pair) { return .conflicted }
        if pair == "??" { return .untracked }
        if index == "D" || workTree == "D" { return .deleted }
        if index == "R" || workTree == "R" { return .renamed }
        if index == "C" || workTree == "C" { return .copied }
        if index == "A" || workTree == "A" { return .added }
        if index == "T" || workTree == "T" { return .typeChanged }
        return .modified
    }
}
