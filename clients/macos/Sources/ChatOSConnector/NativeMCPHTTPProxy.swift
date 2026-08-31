import Foundation

enum NativeMCPHTTPProxy {
    private static let runtimeHeader = "x-local-connector-inline-mcp-runtime"
    private static let resourceHeader = "x-plugin-management-resource-id"
    private static let maximumResponseBytes = 16 * 1_024 * 1_024

    static func forward(request relay: NativeRelayRequest) async throws -> NativeJSONValue {
        guard relay.header(resourceHeader) != nil,
              let encodedRuntime = relay.header(runtimeHeader),
              let decodedRuntime = encodedRuntime.removingPercentEncoding,
              let runtimeData = decodedRuntime.data(using: .utf8) else {
            throw NativeMCPHTTPProxyError.invalidRuntime
        }
        let runtime = try JSONDecoder().decode(Runtime.self, from: runtimeData)
        let url = try validatedURL(runtime.url)
        let method = try rpcMethod(relay.body)
        guard method == "tools/list" || method == "tools/call" else {
            throw NativeMCPHTTPProxyError.unsupportedMethod(method)
        }
        guard runtime.headers.count <= 64,
              runtime.headers.reduce(0, { $0 + $1.key.utf8.count + $1.value.utf8.count }) <= 32 * 1_024 else {
            throw NativeMCPHTTPProxyError.invalidHeaders
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.timeoutInterval = TimeInterval(min(max(runtime.timeoutMilliseconds, 300), 120_000)) / 1_000
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        for (name, value) in runtime.headers {
            guard validHeader(name: name, value: value) else {
                throw NativeMCPHTTPProxyError.invalidHeaders
            }
            request.setValue(value, forHTTPHeaderField: name)
        }
        request.httpBody = try JSONEncoder().encode(relay.body)

        let (data, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse,
              (200..<300).contains(http.statusCode) else {
            throw NativeMCPHTTPProxyError.requestFailed(
                (response as? HTTPURLResponse)?.statusCode ?? 0
            )
        }
        guard data.count <= maximumResponseBytes else {
            throw NativeMCPHTTPProxyError.responseTooLarge
        }
        return try JSONDecoder().decode(NativeJSONValue.self, from: data)
    }

    private static func validatedURL(_ value: String) throws -> URL {
        guard let components = URLComponents(string: value.trimmingCharacters(in: .whitespacesAndNewlines)),
              components.user == nil,
              components.password == nil,
              components.fragment == nil,
              let host = components.host,
              let url = components.url else { throw NativeMCPHTTPProxyError.invalidURL }
        switch components.scheme?.lowercased() {
        case "https": return url
        case "http" where isLoopback(host): return url
        default: throw NativeMCPHTTPProxyError.invalidURL
        }
    }

    private static func isLoopback(_ host: String) -> Bool {
        let normalized = host.lowercased()
        return normalized == "localhost"
            || normalized == "127.0.0.1"
            || normalized == "::1"
            || normalized.hasPrefix("127.")
    }

    private static func rpcMethod(_ value: NativeJSONValue) throws -> String {
        guard case let .object(root) = value,
              case let .string(method)? = root["method"] else {
            throw NativeMCPHTTPProxyError.invalidRequest
        }
        return method
    }

    private static func validHeader(name: String, value: String) -> Bool {
        let normalized = name.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !normalized.isEmpty,
              normalized.unicodeScalars.allSatisfy({ CharacterSet.alphanumerics.contains($0) || "-_".unicodeScalars.contains($0) }),
              !value.contains("\r"), !value.contains("\n") else { return false }
        return ![
            "accept", "connection", "content-length", "content-type", "host",
            "proxy-authenticate", "proxy-authorization", "te", "trailer",
            "transfer-encoding", "upgrade", runtimeHeader, resourceHeader,
        ].contains(normalized)
    }
}

private struct Runtime: Decodable {
    var url: String
    var headers: [String: String]
    var timeoutMilliseconds: Int

    enum CodingKeys: String, CodingKey {
        case url, headers
        case timeoutMilliseconds = "timeout_ms"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        url = try container.decode(String.self, forKey: .url)
        headers = try container.decodeIfPresent([String: String].self, forKey: .headers) ?? [:]
        timeoutMilliseconds = try container.decodeIfPresent(Int.self, forKey: .timeoutMilliseconds) ?? 30_000
    }
}

private enum NativeMCPHTTPProxyError: LocalizedError {
    case invalidRuntime
    case invalidURL
    case invalidHeaders
    case invalidRequest
    case unsupportedMethod(String)
    case requestFailed(Int)
    case responseTooLarge

    var errorDescription: String? {
        switch self {
        case .invalidRuntime: "本机 HTTP MCP 运行配置无效"
        case .invalidURL: "HTTP MCP 必须使用 HTTPS，或指向本机回环地址"
        case .invalidHeaders: "HTTP MCP 请求头无效"
        case .invalidRequest: "HTTP MCP JSON-RPC 请求无效"
        case let .unsupportedMethod(method): "暂不支持 HTTP MCP 方法：\(method)"
        case let .requestFailed(status): "HTTP MCP 请求失败（HTTP \(status)）"
        case .responseTooLarge: "HTTP MCP 返回内容超过大小限制"
        }
    }
}
