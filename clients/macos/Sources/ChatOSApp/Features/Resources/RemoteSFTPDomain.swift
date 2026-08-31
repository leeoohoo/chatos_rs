import ChatOSCore
import Foundation

struct LocalFileEntry: Identifiable, Sendable, Equatable {
    var id: String { url.path }
    let url: URL
    let name: String
    let isDirectory: Bool
    let size: Int64?
    let modifiedAt: Date?
}

enum RemoteSFTPOverwriteAction: Identifiable {
    case upload(LocalFileEntry)
    case download(RemoteFileEntry)

    var id: String {
        switch self {
        case let .upload(entry): "upload-\(entry.id)"
        case let .download(entry): "download-\(entry.id)"
        }
    }

    func title(language: ChatOSLanguage) -> String {
        switch self {
        case .upload: language == .english ? "Overwrite Remote File?" : "覆盖远端文件？"
        case .download: language == .english ? "Overwrite Local File?" : "覆盖本机文件？"
        }
    }

    func message(language: ChatOSLanguage) -> String {
        switch self {
        case let .upload(entry): language == .english
            ? "\(entry.name) already exists in the remote folder."
            : "远端目录中已经存在 \(entry.name)。"
        case let .download(entry): language == .english
            ? "\(entry.name) already exists in the local folder."
            : "本机目录中已经存在 \(entry.name)。"
        }
    }
}
