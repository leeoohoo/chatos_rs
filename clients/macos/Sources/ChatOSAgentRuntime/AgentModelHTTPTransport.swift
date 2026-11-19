import ChatOSNetworking
import Foundation

struct AgentModelHTTPTransport: Sendable {
    typealias DataTransport = @Sendable (URLRequest) async throws -> (Data, Int)
    typealias StreamTransport = @Sendable (URLRequest) async throws -> AgentHTTPStreamResponse
    private typealias FileTransport = @Sendable (
        URLRequest,
        AgentModelRequestBodyFile
    ) async throws -> (Data, Int)
    private typealias FileStreamTransport = @Sendable (
        URLRequest,
        AgentModelRequestBodyFile
    ) async throws -> AgentHTTPStreamResponse

    private let data: DataTransport
    private let stream: StreamTransport
    private let file: FileTransport
    private let fileStream: FileStreamTransport

    init(data override: DataTransport?, stream streamOverride: StreamTransport?) {
        let data: DataTransport = override ?? { request in
            let (data, response) = try await BoundedURLSessionDataLoader.load(
                request: request,
                maximumBytes: 2 * 1_024 * 1_024
            )
            guard let response = response as? HTTPURLResponse else {
                throw AgentRuntimeError.invalidResponse
            }
            return (data, response.statusCode)
        }
        self.data = data
        self.file = if override == nil {
            AgentModelRequestBodyTransport.upload
        } else {
            { request, bodyFile in
                try await data(AgentModelRequestBodyTransport.materialize(
                    request: request,
                    bodyFile: bodyFile
                ))
            }
        }

        let stream = streamOverride ?? AgentURLSessionStream.open
        self.stream = stream
        self.fileStream = if streamOverride == nil {
            AgentModelRequestBodyTransport.stream
        } else {
            { request, bodyFile in
                try await stream(AgentModelRequestBodyTransport.materialize(
                    request: request,
                    bodyFile: bodyFile
                ))
            }
        }
    }

    func send(
        _ request: URLRequest,
        bodyFile: AgentModelRequestBodyFile?
    ) async throws -> (Data, Int) {
        if let bodyFile { return try await file(request, bodyFile) }
        return try await data(request)
    }

    func openStream(
        _ request: URLRequest,
        bodyFile: AgentModelRequestBodyFile?
    ) async throws -> AgentHTTPStreamResponse {
        if let bodyFile { return try await fileStream(request, bodyFile) }
        return try await stream(request)
    }
}
