import ChatOSNetworking
import Foundation

public struct HTTPRequestBodyFile: Sendable, Equatable {
    public let url: URL
    public let byteCount: Int
    public let deleteWhenDone: Bool

    private let cleanupOwner: CleanupOwner?

    private final class CleanupOwner: @unchecked Sendable {
        private let lock = NSLock()
        private let cleanup: @Sendable () -> Void
        private var removed = false

        init(cleanup: @escaping @Sendable () -> Void) {
            self.cleanup = cleanup
        }

        deinit {
            removeIfNeeded()
        }

        func removeIfNeeded() {
            lock.lock()
            guard !removed else {
                lock.unlock()
                return
            }
            removed = true
            lock.unlock()
            cleanup()
        }
    }

    public init(url: URL, byteCount: Int, deleteWhenDone: Bool = false) {
        self.url = url
        self.byteCount = byteCount
        self.deleteWhenDone = deleteWhenDone
        cleanupOwner = deleteWhenDone
            ? CleanupOwner { try? FileManager.default.removeItem(at: url) }
            : nil
    }

    init(
        url: URL,
        byteCount: Int,
        deleteWhenDone: Bool,
        cleanup: @escaping @Sendable () -> Void
    ) {
        self.url = url
        self.byteCount = byteCount
        self.deleteWhenDone = deleteWhenDone
        cleanupOwner = deleteWhenDone ? CleanupOwner(cleanup: cleanup) : nil
    }

    public func removeIfNeeded() {
        cleanupOwner?.removeIfNeeded()
    }

    public static func == (lhs: HTTPRequestBodyFile, rhs: HTTPRequestBodyFile) -> Bool {
        lhs.url == rhs.url
            && lhs.byteCount == rhs.byteCount
            && lhs.deleteWhenDone == rhs.deleteWhenDone
    }
}

public struct HTTPRequest: Sendable {
    public var url: URL
    public var method: String
    public var headers: [String: String]
    public var body: Data?
    public var bodyFile: HTTPRequestBodyFile?
    public var timeoutInterval: TimeInterval?
    public var maximumResponseBytes: Int?
    public var transferTaskID: String?
    public var transferOwnerSessionID: String?

    public init(
        url: URL,
        method: String,
        headers: [String: String] = [:],
        body: Data? = nil,
        bodyFile: HTTPRequestBodyFile? = nil,
        timeoutInterval: TimeInterval? = nil,
        maximumResponseBytes: Int? = nil,
        transferTaskID: String? = nil,
        transferOwnerSessionID: String? = nil
    ) {
        self.url = url
        self.method = method
        self.headers = headers
        self.body = body
        self.bodyFile = bodyFile
        self.timeoutInterval = timeoutInterval
        self.maximumResponseBytes = maximumResponseBytes
        self.transferTaskID = transferTaskID
        self.transferOwnerSessionID = transferOwnerSessionID
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
    func sendFileBacked(_ request: HTTPRequest) async throws -> HTTPResponse
    func stream(_ request: HTTPRequest) async throws -> HTTPStreamResponse
}

public extension HTTPTransport {
    /// Source-compatible fallback for tests and legacy transports. Production URLSession
    /// overrides this method and uploads the file without materializing it as `Data`.
    func sendFileBacked(_ request: HTTPRequest) async throws -> HTTPResponse {
        guard let bodyFile = request.bodyFile, request.body == nil else {
            return try await send(request)
        }
        let values = try bodyFile.url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        guard bodyFile.url.isFileURL,
              values.isRegularFile == true,
              values.fileSize == bodyFile.byteCount,
              bodyFile.byteCount > 0 else {
            throw ChatOSAPIError.invalidResponse
        }
        var materialized = request
        materialized.body = try Data(contentsOf: bodyFile.url, options: .mappedIfSafe)
        materialized.bodyFile = nil
        return try await send(materialized)
    }

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
    private static let largeDownloadThresholdBytes = 2 * 1_024 * 1_024

    private let session: URLSession
    private let largeTransferGovernor: ClientLargeTransferGovernor

    public init(
        session: URLSession = .shared,
        largeTransferGovernor: ClientLargeTransferGovernor = .shared
    ) {
        self.session = session
        self.largeTransferGovernor = largeTransferGovernor
    }

    public func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        var urlRequest = urlRequest(for: request)
        urlRequest.httpBody = request.body

        let data: Data
        let response: URLResponse
        if let maximumResponseBytes = request.maximumResponseBytes {
            do {
                (data, response) = try await BoundedURLSessionDataLoader.load(
                request: urlRequest,
                session: session,
                maximumBytes: maximumResponseBytes
                )
            } catch is BoundedURLSessionDataLoaderError {
                throw ChatOSAPIError.invalidResponse
            }
        } else {
            (data, response) = try await session.data(for: urlRequest)
        }
        return try self.response(data: data, response: response)
    }

    public func sendFileBacked(_ request: HTTPRequest) async throws -> HTTPResponse {
        guard let bodyFile = request.bodyFile, request.body == nil else {
            return try await send(request)
        }
        let values = try bodyFile.url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        guard bodyFile.url.isFileURL,
              values.isRegularFile == true,
              values.fileSize == bodyFile.byteCount,
              bodyFile.byteCount > 0 else {
            throw ChatOSAPIError.invalidResponse
        }
        let transferPermit = try await largeTransferGovernor.acquire(context: .init(
            taskID: request.transferTaskID
                ?? "http-upload:\(UUID().uuidString.lowercased())",
            ownerSessionID: request.transferOwnerSessionID,
            direction: .upload
        ))
        defer { transferPermit.release() }
        let urlRequest = urlRequest(for: request)
        let data: Data
        let response: URLResponse
        if let maximumResponseBytes = request.maximumResponseBytes {
            do {
                (data, response) = try await BoundedURLSessionDataLoader.upload(
                    request: urlRequest,
                    fileURL: bodyFile.url,
                    session: session,
                    maximumBytes: maximumResponseBytes,
                    transferPermit: transferPermit
                )
            } catch is BoundedURLSessionDataLoaderError {
                throw ChatOSAPIError.invalidResponse
            }
        } else {
            (data, response) = try await session.upload(for: urlRequest, fromFile: bodyFile.url)
        }
        return try self.response(data: data, response: response)
    }

    public func stream(_ request: HTTPRequest) async throws -> HTTPStreamResponse {
        let transferPermit: ClientLargeTransferPermit?
        if let maximumResponseBytes = request.maximumResponseBytes,
           maximumResponseBytes > Self.largeDownloadThresholdBytes {
            transferPermit = try await largeTransferGovernor.acquire(context: .init(
                taskID: request.transferTaskID
                    ?? "http-download:\(UUID().uuidString.lowercased())",
                ownerSessionID: request.transferOwnerSessionID,
                direction: .download
            ))
        } else {
            transferPermit = nil
        }
        var urlRequest = urlRequest(for: request)
        urlRequest.httpBody = request.body
        let bytes: URLSession.AsyncBytes
        let response: URLResponse
        do {
            (bytes, response) = try await session.bytes(for: urlRequest)
        } catch {
            transferPermit?.release()
            throw error
        }
        guard let response = response as? HTTPURLResponse else {
            transferPermit?.release()
            throw ChatOSAPIError.invalidResponse
        }
        let headers = response.allHeaderFields.reduce(into: [String: String]()) { result, entry in
            result[String(describing: entry.key).lowercased()] = String(describing: entry.value)
        }
        let stream = URLSessionChunkStream.body(
            from: bytes,
            transferPermit: transferPermit
        )
        return .init(statusCode: response.statusCode, headers: headers, body: stream)
    }

    private func urlRequest(for request: HTTPRequest) -> URLRequest {
        var value = URLRequest(url: request.url)
        value.httpMethod = request.method
        if let timeoutInterval = request.timeoutInterval { value.timeoutInterval = timeoutInterval }
        request.headers.forEach { value.setValue($1, forHTTPHeaderField: $0) }
        return value
    }

    private func response(data: Data, response: URLResponse) throws -> HTTPResponse {
        guard let httpResponse = response as? HTTPURLResponse else {
            throw ChatOSAPIError.invalidResponse
        }
        let headers = httpResponse.allHeaderFields.reduce(into: [String: String]()) { result, entry in
            result[String(describing: entry.key).lowercased()] = String(describing: entry.value)
        }
        return HTTPResponse(statusCode: httpResponse.statusCode, headers: headers, body: data)
    }
}
