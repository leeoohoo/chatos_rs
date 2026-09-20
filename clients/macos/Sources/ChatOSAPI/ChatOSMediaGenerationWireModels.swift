import ChatOSCore
import Foundation

enum VideoWireProtocol { case openAICompatible, miniMaxNative, volcengineArk }

struct RuntimeModelConfig: Decodable, Sendable {
    var provider: String?
    var model: String
    var apiKey: String?
    var baseURL: String?
    var enabled: Bool

    enum CodingKeys: String, CodingKey {
        case provider, model, enabled
        case apiKey = "api_key"
        case baseURL = "base_url"
    }

    var videoProtocol: VideoWireProtocol {
        switch provider?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case "gpt", "openai": return .openAICompatible
        case "minimax": return .miniMaxNative
        case "volcengine", "ark", "doubao", "seedance", "bytedance", "byteplus":
            return .volcengineArk
        default:
            let host = URL(string: baseURL ?? "")?.host?.lowercased()
            if ["api.minimax.io", "api.minimaxi.com"].contains(host ?? "") {
                return .miniMaxNative
            }
            if host == "ark.cn-beijing.volces.com"
                || host?.hasSuffix(".volcengineapi.com") == true {
                return .volcengineArk
            }
            return .openAICompatible
        }
    }
}

struct ProviderImageResponse: Decodable, Sendable {
    var id: String?
    var model: String?
    var data: [ProviderImageItem]
}

struct ProviderImageItem: Decodable, Sendable {
    var id: String?
    var b64JSON: String?
    var base64Data: String?
    var url: URL?
    var mimeType: String?
    var revisedPrompt: String?

    enum CodingKeys: String, CodingKey {
        case id
        case b64JSON = "b64_json"
        case base64Data = "base64"
        case url
        case mimeType = "mime_type"
        case revisedPrompt = "revised_prompt"
    }
}

enum ProviderImageOperation: String {
    case generation = "generations"
    case edit = "edits"
}

struct ProviderVideoJob: Sendable {
    var id: String
    var status: String
    var progress: Double?
    var model: String?
    var errorMessage: String?
    var contentURL: URL?

    var isPending: Bool {
        status == "queued" || status == "in_progress" || status == "processing"
    }
}

enum MediaGenerationClientError: LocalizedError, MediaGenerationSubmissionFailure {
    case preflightFailed(String)
    case invalidModelConfiguration
    case invalidInputImage
    case invalidProviderResponse
    case responseTooLarge
    case providerRejected(statusCode: Int, detail: String)
    case invalidVideoOptions
    case invalidVideoContent
    case videoTimedOut
    case videoFailed(String?)
    case invalidMiniMaxPrompt
    case invalidMiniMaxImageDimensions
    case unsupportedLastFrameProtocol
    case invalidReferenceVideo
    case unsupportedReferenceVideoProtocol
    case mixedFrameAndReferenceVideoInputs
    case missingReferenceVideo
    case videoEndpointReturnedHTML(String)

    var errorDescription: String? {
        switch self {
        case let .preflightFailed(detail):
            detail
        case .invalidModelConfiguration:
            "模型缺少可用的 API 地址或密钥，请检查模型配置。"
        case .invalidInputImage:
            "参考图无效或超过 20 MB。"
        case .invalidProviderResponse:
            "模型返回的数据格式无法识别。"
        case .responseTooLarge:
            "模型返回的数据过大。"
        case let .providerRejected(statusCode, detail):
            "模型请求失败（HTTP \(statusCode)）：\(detail)"
        case .invalidVideoOptions:
            "视频尺寸或时长不受当前协议支持。"
        case .invalidMiniMaxPrompt:
            "MiniMax 视频提示词不能为空，且不能超过 7000 字符。"
        case .invalidMiniMaxImageDimensions:
            "MiniMax 参考图宽高须为 256–5760 像素，宽高比须为 0.4–2.5。"
        case .unsupportedLastFrameProtocol:
            "尾帧约束需要同时提供首帧，且当前视频模型必须支持首尾帧生成。"
        case .invalidReferenceVideo:
            "参考视频必须是有效的 MP4 或 MOV，且文件不能超过 47 MB。"
        case .unsupportedReferenceVideoProtocol:
            "当前视频模型或接口不支持使用上一段视频作为参考。"
        case .mixedFrameAndReferenceVideoInputs:
            "上一段视频参考不能与首帧或尾帧同时发送，请重新选择视频衔接方式。"
        case .missingReferenceVideo:
            "视频编辑或延续需要先提供一段原视频。"
        case let .videoEndpointReturnedHTML(endpoint):
            "视频接口 \(endpoint) 返回了网页而非任务数据，请检查客户端协议与接口路径是否匹配。"
        case .invalidVideoContent:
            "模型没有返回可播放的视频文件。"
        case .videoTimedOut:
            "视频生成等待超时，可以稍后重新尝试。"
        case let .videoFailed(detail):
            "视频生成失败：\(Self.nonEmpty(detail) ?? "模型未提供失败原因")"
        }
    }

    var requestMayHaveBeenSubmitted: Bool {
        switch self {
        case .preflightFailed, .invalidModelConfiguration, .invalidInputImage,
             .invalidVideoOptions, .invalidMiniMaxPrompt, .invalidMiniMaxImageDimensions,
             .unsupportedLastFrameProtocol, .invalidReferenceVideo,
             .unsupportedReferenceVideoProtocol, .mixedFrameAndReferenceVideoInputs,
             .missingReferenceVideo:
            false
        case .invalidProviderResponse, .responseTooLarge, .providerRejected,
             .invalidVideoContent, .videoTimedOut, .videoFailed, .videoEndpointReturnedHTML:
            true
        }
    }

    private static func nonEmpty(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? nil : trimmed
    }
}
