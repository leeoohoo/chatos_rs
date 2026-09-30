import Foundation

enum NativeWorkspaceFilesystemError: LocalizedError {
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

    var errorDescription: String? {
        switch self {
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
