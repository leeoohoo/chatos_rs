import Foundation

struct NativeWorkspaceDirectoryBody: Decodable, Sendable {
    var path: String?
}

struct NativeWorkspaceFilesystemRequest: Decodable, Sendable {
    var operation: String
    var path: String?
    var query: String?
    var limit: Int?
    var content: String?
    var recursive: Bool?
    var sourcePath: String?
    var targetPath: String?
    var replaceExisting: Bool?

    enum CodingKeys: String, CodingKey {
        case operation, path, query, limit, content, recursive
        case sourcePath = "source_path"
        case targetPath = "target_path"
        case replaceExisting = "replace_existing"
    }
}

enum NativeWorkspaceRelayError: LocalizedError {
    case invalidContext
    case unsupportedRequest
    case unsupportedOperation(String)
    case missingField(String)
    case unsafePath
    case rootMutation
    case notFound
    case notDirectory
    case notFile
    case symbolicLink
    case alreadyExists
    case directoryNotEmpty
    case fileTooLarge(Int64)
    case invalidResponse

    var errorDescription: String? {
        switch self {
        case .invalidContext: "Relay 请求与当前设备或工作区不匹配"
        case .unsupportedRequest: "不支持的工作区 Relay 请求"
        case let .unsupportedOperation(operation): "不支持的文件系统操作：\(operation)"
        case let .missingField(field): "文件系统请求缺少字段：\(field)"
        case .unsafePath: "路径无效或超出授权工作区"
        case .rootMutation: "不能修改授权工作区根目录"
        case .notFound: "文件或目录不存在"
        case .notDirectory: "目标不是目录"
        case .notFile: "目标不是普通文件"
        case .symbolicLink: "操作不能穿过符号链接"
        case .alreadyExists: "目标已经存在"
        case .directoryNotEmpty: "目录不为空，请使用递归删除"
        case let .fileTooLarge(size): "文件过大，无法预览（\(size) 字节，最大 2097152 字节）"
        case .invalidResponse: "无法编码工作区 Relay 响应"
        }
    }

    var status: Int {
        switch self {
        case .notFound: 404
        case .alreadyExists, .directoryNotEmpty: 409
        case .fileTooLarge: 413
        default: 400
        }
    }
}
