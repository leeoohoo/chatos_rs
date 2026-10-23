import AppKit
import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
import ChatOSCore
import ChatOSMediaWorkerProtocol
import Foundation
import UniformTypeIdentifiers

struct NativePetLocalFileService: ProjectFilesystemServicing, Sendable {
    private static let maximumTextPreviewBytes: Int64 = 2 * 1_024 * 1_024
    static let maximumImagePreviewBytes = Int64(
        ChatOSMediaWorkerProtocol.maximumNormalizedImageBytes
    )
    static let maximumImagePreviewPixels = Int64(
        ChatOSMediaWorkerProtocol.maximumNormalizedImagePixels
    )
    private static let imageExtensions: Set<String> = [
        "avif", "bmp", "gif", "heic", "heif", "ico", "jpeg", "jpg",
        "png", "svg", "tif", "tiff", "webp",
    ]

    private let codeReadWorker: any CodeReadWorkerExecuting

    init(
        codeReadWorker: any CodeReadWorkerExecuting = CodeReadWorkerProcessClient.shared
    ) {
        self.codeReadWorker = codeReadWorker
    }

    func readFile(path: String) async throws -> ProjectFileContent {
        let metadata = try await Self.inspectFile(path)
        if metadata.isSourceBackedImage {
            return ProjectFileContent(
                path: metadata.url.path,
                displayPath: metadata.url.path,
                name: metadata.url.lastPathComponent,
                contentType: metadata.contentType,
                isBinary: true,
                isWritable: metadata.isWritable,
                size: metadata.size,
                modifiedAt: metadata.modifiedAt,
                content: "",
                sourceFileURL: metadata.url
            )
        }

        do {
            let canonicalURL = metadata.url.resolvingSymlinksInPath().standardizedFileURL
            let result = try await codeReadWorker.execute(
                operation: "read_file_raw",
                arguments: [
                    "path": .string(canonicalURL.lastPathComponent),
                    "with_line_numbers": .bool(false),
                ],
                projectRoot: canonicalURL.deletingLastPathComponent(),
                requestCWD: nil,
                defaultToolRoot: nil,
                ripgrepURL: nil
            )
            guard case let .object(payload) = result,
                  case let .string(content)? = payload["content"] else {
                throw CodeReadWorkerProcessClientError.invalidResponse
            }
            let workerSize: Int64
            if case let .number(value)? = payload["size_bytes"], value.isFinite {
                workerSize = Int64(value)
            } else {
                workerSize = metadata.size
            }
            return ProjectFileContent(
                path: metadata.url.path,
                displayPath: metadata.url.path,
                name: metadata.url.lastPathComponent,
                contentType: metadata.contentType,
                isBinary: false,
                isWritable: metadata.isWritable,
                size: workerSize,
                modifiedAt: metadata.modifiedAt,
                content: content
            )
        } catch let error as CodeReadWorkerProcessClientError {
            guard case let .requestRejected(code, _) = error, code == "binary_file" else {
                throw error
            }
            return ProjectFileContent(
                path: metadata.url.path,
                displayPath: metadata.url.path,
                name: metadata.url.lastPathComponent,
                contentType: metadata.contentType,
                isBinary: true,
                isWritable: metadata.isWritable,
                size: metadata.size,
                modifiedAt: metadata.modifiedAt,
                content: ""
            )
        }
    }

    func writeFile(path: String, content: String) async throws {
        let task = Task.detached {
            try Task.checkCancellation()
            let url = try Self.validatedFileURL(path)
            guard FileManager.default.isWritableFile(atPath: url.path) else {
                throw NativePetLocalFileError.notWritable
            }
            try Task.checkCancellation()
            try Data(content.utf8).write(to: url, options: .atomic)
        }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
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

    private struct FileMetadata: Sendable {
        let url: URL
        let contentType: String?
        let isSourceBackedImage: Bool
        let isWritable: Bool
        let size: Int64
        let modifiedAt: Date?
    }

    private static func inspectFile(_ path: String) async throws -> FileMetadata {
        let task = Task.detached(priority: .userInitiated) {
            try Task.checkCancellation()
            let url = try validatedFileURL(path)
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
                || imageExtensions.contains(fileExtension)
            let isSourceBackedImage = isImage && fileExtension != "svg"
            let maximumBytes = isSourceBackedImage
                ? maximumImagePreviewBytes
                : maximumTextPreviewBytes
            guard size <= maximumBytes else {
                throw NativePetLocalFileError.fileTooLarge(size, maximumBytes)
            }
            return FileMetadata(
                url: url,
                contentType: values.contentType?.preferredMIMEType,
                isSourceBackedImage: isSourceBackedImage,
                isWritable: FileManager.default.isWritableFile(atPath: url.path),
                size: size,
                modifiedAt: values.contentModificationDate
            )
        }
        return try await withTaskCancellationHandler {
            try await task.value
        } onCancel: {
            task.cancel()
        }
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
