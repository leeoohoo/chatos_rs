import Foundation

public enum ChatOSAPIError: Error, Sendable, Equatable {
    case invalidBaseURL
    case invalidEndpoint
    case invalidResponse
    case invalidCredentials
    case unauthorized
    case server(statusCode: Int, message: String)
    case serverDetail(statusCode: Int, message: String, code: String?, challengePrompt: String?)
    case decoding(String)
    case missingWebSocketTicket
    case missingModelConfiguration
}

extension ChatOSAPIError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .invalidBaseURL, .invalidEndpoint:
            "服务地址无效。"
        case .invalidResponse:
            "服务器返回了无法识别的响应。"
        case .invalidCredentials:
            "请输入账号和密码。"
        case .unauthorized:
            "账号或密码错误，或登录状态已过期。"
        case let .server(_, message):
            message.isEmpty ? "服务器请求失败。" : message
        case let .serverDetail(_, message, _, _):
            message.isEmpty ? "服务器请求失败。" : message
        case .decoding:
            "服务器数据格式与客户端不一致。"
        case .missingWebSocketTicket:
            "无法建立实时连接，请稍后重试。"
        case .missingModelConfiguration:
            "当前会话没有可用的模型配置，请先在设置中选择模型。"
        }
    }
}
