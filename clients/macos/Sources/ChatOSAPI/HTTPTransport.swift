import Foundation

public struct HTTPRequest: Sendable {
    public var url: URL
    public var method: String
    public var headers: [String: String]
    public var body: Data?
    public var timeoutInterval: TimeInterval?

    public init(
        url: URL,
        method: String,
        headers: [String: String] = [:],
        body: Data? = nil,
        timeoutInterval: TimeInterval? = nil
    ) {
        self.url = url
        self.method = method
        self.headers = headers
        self.body = body
        self.timeoutInterval = timeoutInterval
    }
}

public struct HTTPResponse: Sendable {
    public var statusCode: Int
    public var headers: [String: String]
    public var body: Data

    public init(statusCode: Int, headers: [String: String], body: Data) {
        self.statusCode = statusCode
        self.headers = headers
        self.body = body
    }
}

public struct HTTPStreamResponse: Sendable {
    public var statusCode: Int
    public var headers: [String: String]
    public var body: AsyncThrowingStream<Data, Error>

    public init(statusCode: Int, headers: [String: String], body: AsyncThrowingStream<Data, Error>) {
        self.statusCode = statusCode; self.headers = headers; self.body = body
    }
}

public protocol HTTPTransport: Sendable {
    func send(_ request: HTTPRequest) async throws -> HTTPResponse
    func stream(_ request: HTTPRequest) async throws -> HTTPStreamResponse
}

public extension HTTPTransport {
    /// Test and legacy transports remain source-compatible. The production URLSession transport
    /// overrides this with a true byte stream.
    func stream(_ request: HTTPRequest) async throws -> HTTPStreamResponse {
        let response = try await send(request)
        return .init(statusCode: response.statusCode, headers: response.headers,
                     body: AsyncThrowingStream { continuation in
                         continuation.yield(response.body); continuation.finish()
                     })
    }
}

public struct URLSessionHTTPTransport: HTTPTransport {
    private let session: URLSession

    public init(session: URLSession = .shared) {
        self.session = session
    }

    public func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        var urlRequest = URLRequest(url: request.url)
        urlRequest.httpMethod = request.method
        urlRequest.httpBody = request.body
        if let timeoutInterval = request.timeoutInterval {
            urlRequest.timeoutInterval = timeoutInterval
        }
        request.headers.forEach { urlRequest.setValue($1, forHTTPHeaderField: $0) }

        let (data, response) = try await session.data(for: urlRequest)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw ChatOSAPIError.invalidResponse
        }

        let headers = httpResponse.allHeaderFields.reduce(into: [String: String]()) { result, entry in
            result[String(describing: entry.key).lowercased()] = String(describing: entry.value)
        }
        return HTTPResponse(
            statusCode: httpResponse.statusCode,
            headers: headers,
            body: data
        )
    }

    public func stream(_ request: HTTPRequest) async throws -> HTTPStreamResponse {
        var urlRequest = URLRequest(url: request.url)
        urlRequest.httpMethod = request.method; urlRequest.httpBody = request.body
        if let timeoutInterval = request.timeoutInterval { urlRequest.timeoutInterval = timeoutInterval }
        request.headers.forEach { urlRequest.setValue($1, forHTTPHeaderField: $0) }
        let (bytes, response) = try await session.bytes(for: urlRequest)
        guard let response = response as? HTTPURLResponse else { throw ChatOSAPIError.invalidResponse }
        let headers = response.allHeaderFields.reduce(into: [String: String]()) { result, entry in
            result[String(describing: entry.key).lowercased()] = String(describing: entry.value)
        }
        let stream = AsyncThrowingStream<Data, Error> { continuation in
            let task = Task {
                do {
                    var chunk = Data(); chunk.reserveCapacity(4_096)
                    for try await byte in bytes {
                        chunk.append(byte)
                        if byte == 10 || chunk.count >= 4_096 {
                            continuation.yield(chunk); chunk.removeAll(keepingCapacity: true)
                        }
                    }
                    if !chunk.isEmpty { continuation.yield(chunk) }
                    continuation.finish()
                } catch { continuation.finish(throwing: error) }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        return .init(statusCode: response.statusCode, headers: headers, body: stream)
    }
}
