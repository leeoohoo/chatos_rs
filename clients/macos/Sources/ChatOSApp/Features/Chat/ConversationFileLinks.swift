import AppKit
import Darwin
import Foundation
import SwiftUI
import UniformTypeIdentifiers

enum ConversationFileLinkError: LocalizedError {
    case invalidPath, unavailable, tooLarge

    var errorDescription: String? {
        switch self {
        case .invalidPath: "文件链接不在当前会话工作目录内。"
        case .unavailable: "文件不存在或无法读取，请检查任务输出路径。"
        case .tooLarge: "文件超过可导出的大小限制（128 MB）。"
        }
    }
}

enum ConversationFileLinkReader {
    static func relativePath(_ url: URL, root: URL) throws -> String {
        guard url.scheme == nil || url.isFileURL,
              url.host == nil || url.host == "" || url.host == "localhost",
              url.query == nil else { throw ConversationFileLinkError.invalidPath }
        // URL.path is already percent-decoded: decoding again corrupts literal '%' names.
        let path = url.path
        guard !path.isEmpty, !path.contains("\0"), !path.contains("\\") else {
            throw ConversationFileLinkError.invalidPath
        }
        let relative: String
        if path.hasPrefix("/") {
            let prefix = root.standardizedFileURL.path + "/"
            guard path.hasPrefix(prefix) else { throw ConversationFileLinkError.invalidPath }
            relative = String(path.dropFirst(prefix.count))
        } else {
            relative = path
        }
        let components = relative.split(separator: "/").map(String.init)
        guard !components.contains("..") else { throw ConversationFileLinkError.invalidPath }
        let cleaned = components.filter { $0 != "." }
        guard !cleaned.isEmpty else { throw ConversationFileLinkError.invalidPath }
        return cleaned.joined(separator: "/")
    }

    static func read(_ url: URL, root: URL) throws -> (name: String, data: Data) {
        let relative = try relativePath(url, root: root)
        let components = relative.split(separator: "/").map(String.init)
        // Walk descriptor-relative paths; no symlink or directory replacement can escape root.
        var directory = open(root.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard directory >= 0 else { throw ConversationFileLinkError.unavailable }
        defer { close(directory) }
        for component in components.dropLast() {
            let next = openat(directory, component, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
            guard next >= 0 else { throw ConversationFileLinkError.unavailable }
            close(directory)
            directory = next
        }
        let descriptor = openat(directory, components.last!, O_RDONLY | O_NOFOLLOW | O_NONBLOCK | O_CLOEXEC)
        guard descriptor >= 0 else { throw ConversationFileLinkError.unavailable }
        defer { close(descriptor) }
        var info = stat()
        guard fstat(descriptor, &info) == 0, info.st_mode & S_IFMT == S_IFREG else {
            throw ConversationFileLinkError.unavailable
        }
        let maximum = 128 * 1_024 * 1_024
        guard info.st_size <= maximum else { throw ConversationFileLinkError.tooLarge }
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 64 * 1_024)
        while true {
            let count = Darwin.read(descriptor, &buffer, buffer.count)
            if count < 0 {
                if errno == EINTR { continue }
                throw ConversationFileLinkError.unavailable
            }
            if count == 0 { break }
            guard data.count + count <= maximum else { throw ConversationFileLinkError.tooLarge }
            data.append(contentsOf: buffer.prefix(count))
        }
        return (components.last!, data)
    }
}

private struct ConversationFileLinks: ViewModifier {
    @EnvironmentObject private var model: AppModel
    let conversationID: String
    let projectRootPath: String?
    @State private var errorMessage: String?

    func body(content: Content) -> some View {
        content.environment(\.openURL, OpenURLAction { url in
            if ["http", "https", "mailto"].contains(url.scheme?.lowercased() ?? "") {
                return .systemAction
            }
            Task { await export(url) }
            return .handled
        })
        .alert("无法下载文件", isPresented: Binding(
            get: { errorMessage != nil }, set: { if !$0 { errorMessage = nil } }
        )) {
            Button("好") { errorMessage = nil }
        } message: {
            Text(errorMessage ?? "")
        }
    }

    private func export(_ url: URL) async {
        guard let owner = model.authenticatedUserID else { return }
        let projectID = model.projects.first { $0.conversationID == conversationID }?.id
        let logicalRoot = projectRootPath ?? projectID.flatMap { model.workspaceProject(id: $0)?.rootPath }
        do {
            let root = try await model.localConnectorService.conversationFileRoot(
                ownerUserID: owner, conversationID: conversationID, projectRootPath: logicalRoot
            )
            var fileURL = url
            if url.scheme == "local", let logicalRoot,
               let resolved = PetFileLinkResolver.resolve(url, projectRootPath: logicalRoot),
               resolved.path.hasPrefix(logicalRoot + "/") {
                let relative = String(resolved.path.dropFirst(logicalRoot.count + 1))
                fileURL = root.appendingPathComponent(relative)
            }
            let scopedURL = fileURL
            let file = try await AppCancellableDetachedWork.run {
                try ConversationFileLinkReader.read(scopedURL, root: root)
            }
            guard model.authenticatedUserID == owner else { return }
            if url.fragment != nil, logicalRoot != nil,
               model.openPetFileLink(url, projectRootPath: logicalRoot) { return }
            let panel = NSSavePanel()
            panel.nameFieldStringValue = file.name
            panel.canCreateDirectories = true
            if let type = UTType(filenameExtension: URL(fileURLWithPath: file.name).pathExtension) {
                panel.allowedContentTypes = [type]
            }
            guard panel.runModal() == .OK, let destination = panel.url,
                  model.authenticatedUserID == owner else { return }
            try await AppCancellableDetachedWork.run {
                try file.data.write(to: destination, options: .atomic)
            }
        } catch {
            guard model.authenticatedUserID == owner else { return }
            errorMessage = error.localizedDescription
        }
    }
}

extension View {
    func conversationFileLinks(conversationID: String, projectRootPath: String? = nil) -> some View {
        modifier(ConversationFileLinks(conversationID: conversationID, projectRootPath: projectRootPath))
    }
}
