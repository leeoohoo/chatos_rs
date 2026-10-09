import ChatOSNetworking
import Foundation

enum AgentURLSessionStream {
    static func open(_ request: URLRequest) async throws -> AgentHTTPStreamResponse {
        let (bytes, response) = try await URLSession.shared.bytes(for: request)
        guard let response = response as? HTTPURLResponse else {
            throw AgentRuntimeError.invalidResponse
        }
        let headers = response.allHeaderFields.reduce(into: [String: String]()) { result, entry in
            result[String(describing: entry.key).lowercased()] = String(describing: entry.value)
        }
        let body = URLSessionChunkStream.body(from: bytes)
        return .init(statusCode: response.statusCode, headers: headers, body: body)
    }
}
