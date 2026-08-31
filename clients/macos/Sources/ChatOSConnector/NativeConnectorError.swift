import Foundation
import Security

public enum NativeConnectorError: LocalizedError, Sendable {
    case invalidEndpoint
    case keychain(OSStatus)
    case notPaired
    case server(status: Int, message: String)
    case invalidResponse(String)
    case workspaceUnavailable
    case unsafeWorkingDirectory
    case pluginInstallerUnavailable
    case pluginInstallation(String)

    public var errorDescription: String? {
        switch self {
        case .invalidEndpoint: "Local Connector 网关地址无效。"
        case let .keychain(status): "读取本机安全凭据失败（Keychain \(status)）。"
        case .notPaired: "这台设备尚未与 ChatOS 网关配对。"
        case let .server(status, message): "网关请求失败（\(status)）：\(message)"
        case let .invalidResponse(message): "网关返回的数据无法解析：\(message)"
        case .workspaceUnavailable: "没有找到可用的本机工作区。"
        case .unsafeWorkingDirectory: "工作目录超出了当前工作区。"
        case .pluginInstallerUnavailable: "原生 Plugin 安装器尚未完成迁移。"
        case let .pluginInstallation(message): "Plugin 安装失败：\(message)"
        }
    }
}
