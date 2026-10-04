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
        let body = AsyncThrowingStream<Data, Error>(bufferingPolicy: .bufferingOldest(64)) { continuation in
            let task = Task {
                do {
                    var chunk = Data()
                    chunk.reserveCapacity(4_096)
                    for try await byte in bytes {
                        chunk.append(byte)
                        if byte == 10 || chunk.count >= 4_096 {
                            switch continuation.yield(chunk) {
                            case .enqueued:
                                chunk.removeAll(keepingCapacity: true)
                            case .terminated:
                                return
                            case .dropped:
                                continuation.finish(throwing: AgentRuntimeError.invalidResponse)
                                return
                            @unknown default:
                                continuation.finish(throwing: AgentRuntimeError.invalidResponse)
                                return
                            }
                        }
                    }
                    if !chunk.isEmpty {
                        guard case .enqueued = continuation.yield(chunk) else {
                            continuation.finish(throwing: AgentRuntimeError.invalidResponse)
                            return
                        }
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        return .init(statusCode: response.statusCode, headers: headers, body: body)
    }
}
