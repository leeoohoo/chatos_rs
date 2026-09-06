import AppKit
import ChatOSCore
import Foundation
import UniformTypeIdentifiers

struct NativePetLocalFileService: ProjectFilesystemServicing, Sendable {
    private static let maximumTextPreviewBytes: Int64 = 2 * 1_024 * 1_024
    private static let maximumImagePreviewBytes: Int64 = 25 * 1_024 * 1_024
    private static let imageExtensions: Set<String> = [
        "avif", "bmp", "gif", "heic", "heif", "ico", "jpeg", "jpg",
        "png", "svg", "tif", "tiff", "webp",
    ]

    func readFile(path: String) async throws -> ProjectFileContent {
        try await Task.detached {
            let url = try Self.validatedFileURL(path)
            let values = try url.resourceValues(forKeys: [
                .isRegularFileKey,
                .fileSizeKey,
                .contentModificationDateKey,
                .contentTypeKey,
            ])
            guard values.isRegularFile == true else {
                throw NativePetLocalFileError.notFile
            }

            let size = Int64(values.fileSize ?? 0)
            let fileExtension = url.pathExtension.lowercased()
            let isImage = values.contentType?.conforms(to: .image) == true
                || Self.imageExtensions.contains(fileExtension)
            let maximumBytes = isImage
                ? Self.maximumImagePreviewBytes
                : Self.maximumTextPreviewBytes
            guard size <= maximumBytes else {
                throw NativePetLocalFileError.fileTooLarge(size, maximumBytes)
            }

            let data = try Data(contentsOf: url, options: [.mappedIfSafe])
            let isBinary = (isImage && fileExtension != "svg")
                || data.prefix(8_000).contains(0)
            return ProjectFileContent(
                path: url.path,
                displayPath: url.path,
                name: url.lastPathComponent,
                contentType: values.contentType?.preferredMIMEType,
                isBinary: isBinary,
                isWritable: FileManager.default.isWritableFile(atPath: url.path),
                size: size,
                modifiedAt: values.contentModificationDate,
                content: isBinary
                    ? data.base64EncodedString()
                    : String(decoding: data, as: UTF8.self)
            )
        }.value
    }

    func writeFile(path: String, content: String) async throws {
        try await Task.detached {
            let url = try Self.validatedFileURL(path)
            guard FileManager.default.isWritableFile(atPath: url.path) else {
                throw NativePetLocalFileError.notWritable
            }
            try Data(content.utf8).write(to: url, options: .atomic)
        }.value
    }

    func openExternally(path: String, mode: ProjectFileExternalOpenMode) async throws {
        let url = try Self.validatedFileURL(path)
        try await MainActor.run {
            switch mode {
            case .reveal:
                NSWorkspace.shared.activateFileViewerSelecting([url])
            case .default:
                guard NSWorkspace.shared.open(url) else {
                    throw NativePetLocalFileError.openFailed
                }
            case .code:
                let configuration = NSWorkspace.OpenConfiguration()
                NSWorkspace.shared.open(
                    [url],
                    withApplicationAt: URL(fileURLWithPath: "/Applications/Visual Studio Code.app"),
                    configuration: configuration
                )
            }
        }
    }

    func listEntries(path: String, forceRefresh: Bool) async throws -> ProjectDirectoryListing {
        throw NativePetLocalFileError.unsupportedOperation
    }

    func searchEntries(path: String, query: String, limit: Int) async throws -> [ProjectFileEntry] {
        throw NativePetLocalFileError.unsupportedOperation
    }

    func searchContent(path: String, query: String, limit: Int) async throws -> [ProjectFileContentMatch] {
        throw NativePetLocalFileError.unsupportedOperation
    }

    func createFile(parentPath: String, name: String) async throws {
        throw NativePetLocalFileError.unsupportedOperation
    }

    func createDirectory(parentPath: String, name: String) async throws {
        throw NativePetLocalFileError.unsupportedOperation
    }

    func deleteEntry(path: String, recursive: Bool) async throws {
        throw NativePetLocalFileError.unsupportedOperation
    }

    private static func validatedFileURL(_ path: String) throws -> URL {
        let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.hasPrefix("/") else {
            throw NativePetLocalFileError.invalidPath
        }
        let url = URL(fileURLWithPath: trimmed).standardizedFileURL
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory),
              !isDirectory.boolValue else {
            throw NativePetLocalFileError.notFile
        }
        return url
    }
}

private enum NativePetLocalFileError: LocalizedError {
    case invalidPath
    case notFile
    case notWritable
    case fileTooLarge(Int64, Int64)
    case openFailed
    case unsupportedOperation

    var errorDescription: String? {
        switch self {
        case .invalidPath:
            "Finder 传入的文件路径无效"
        case .notFile:
            "文件不存在或不是普通文件"
        case .notWritable:
            "这个文件不可写"
        case let .fileTooLarge(size, maximum):
            "文件过大（\(size) 字节），当前预览上限为 \(maximum) 字节"
        case .openFailed:
            "无法使用默认应用打开文件"
        case .unsupportedOperation:
            "宠物文件台不支持这个文件操作"
        }
    }
}
