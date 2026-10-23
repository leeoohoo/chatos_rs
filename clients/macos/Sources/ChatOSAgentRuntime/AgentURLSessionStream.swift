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

    static func open(
        _ request: URLRequest,
        bodyFileURL: URL,
        transferPermit: ClientLargeTransferPermit? = nil,
        configuration: URLSessionConfiguration = URLSession.shared.configuration
    ) async throws -> AgentHTTPStreamResponse {
        try await AgentFileUploadStreamLoader(transferPermit: transferPermit).open(
            request,
            bodyFileURL: bodyFileURL,
            configuration: configuration
        )
    }
}

private final class AgentFileUploadStreamLoader: NSObject, URLSessionDataDelegate,
    @unchecked Sendable {
    private let lock = NSLock()
    private var responseContinuation: CheckedContinuation<AgentHTTPStreamResponse, any Error>?
    private var bodyBuffer: AgentFileUploadResponseBuffer?
    private var bodyStream: AsyncThrowingStream<Data, Error>?
    private var session: URLSession?
    private var task: URLSessionUploadTask?
    private var responseTaskSuspended = false
    private var responseDelivered = false
    private var completed = false
    private var transferPermit: ClientLargeTransferPermit?

    init(transferPermit: ClientLargeTransferPermit?) {
        self.transferPermit = transferPermit
    }

    func open(
        _ request: URLRequest,
        bodyFileURL: URL,
        configuration: URLSessionConfiguration
    ) async throws -> AgentHTTPStreamResponse {
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                start(
                    request,
                    bodyFileURL: bodyFileURL,
                    configuration: configuration,
                    continuation: continuation
                )
            }
        } onCancel: {
            self.cancel()
        }
    }

    private func start(
        _ request: URLRequest,
        bodyFileURL: URL,
        configuration: URLSessionConfiguration,
        continuation: CheckedContinuation<AgentHTTPStreamResponse, any Error>
    ) {
        let bodyBuffer = AgentFileUploadResponseBuffer(
            onConsumerCancellation: { [weak self] in self?.cancel() },
            onLowWatermark: { [weak self] in self?.resumeResponseTaskIfNeeded() }
        )
        let bodyStream = bodyBuffer.makeStream()
        lock.lock()
        guard !completed else {
            lock.unlock()
            continuation.resume(throwing: CancellationError())
            return
        }
        responseContinuation = continuation
        self.bodyBuffer = bodyBuffer
        self.bodyStream = bodyStream
        let session = URLSession(
            configuration: configuration,
            delegate: self,
            delegateQueue: nil
        )
        let task = session.uploadTask(with: request, fromFile: bodyFileURL)
        self.session = session
        self.task = task
        lock.unlock()
        task.resume()
    }

    private func cancel() {
        finish(error: CancellationError(), cancelTask: true)
    }

    func urlSession(
        _ session: URLSession,
        dataTask: URLSessionDataTask,
        didReceive response: URLResponse,
        completionHandler: @escaping @Sendable (URLSession.ResponseDisposition) -> Void
    ) {
        guard let response = response as? HTTPURLResponse else {
            completionHandler(.cancel)
            finish(error: AgentRuntimeError.invalidResponse, cancelTask: true)
            return
        }
        let headers = response.allHeaderFields.reduce(into: [String: String]()) { result, entry in
            result[String(describing: entry.key).lowercased()] = String(describing: entry.value)
        }
        lock.lock()
        guard !completed, !responseDelivered,
              let continuation = responseContinuation,
              let bodyStream, bodyBuffer != nil else {
            lock.unlock()
            completionHandler(.cancel)
            return
        }
        responseDelivered = true
        responseContinuation = nil
        self.bodyStream = nil
        lock.unlock()
        completionHandler(.allow)
        continuation.resume(returning: .init(
            statusCode: response.statusCode,
            headers: headers,
            body: bodyStream
        ))
    }

    func urlSession(
        _ session: URLSession,
        dataTask: URLSessionDataTask,
        didReceive data: Data
    ) {
        lock.lock()
        let buffer = completed ? nil : bodyBuffer
        lock.unlock()
        guard let buffer else { return }
        var shouldSuspend = false
        var offset = data.startIndex
        while offset < data.endIndex {
            let end = data.index(offset, offsetBy: min(16 * 1_024, data.distance(
                from: offset,
                to: data.endIndex
            )))
            switch buffer.enqueue(Data(data[offset..<end])) {
            case .accepted:
                break
            case .suspend:
                shouldSuspend = true
            case .overflow:
                finish(error: AgentRuntimeError.invalidResponse, cancelTask: true)
                return
            }
            offset = end
        }
        if shouldSuspend { suspendResponseTaskIfNeeded() }
    }

    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didSendBodyData bytesSent: Int64,
        totalBytesSent: Int64,
        totalBytesExpectedToSend: Int64
    ) {
        guard totalBytesExpectedToSend > 0,
              totalBytesSent >= totalBytesExpectedToSend else { return }
        releaseTransferPermit()
    }

    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didCompleteWithError error: (any Error)?
    ) {
        finish(error: error, cancelTask: false)
    }

    private func finish(error: (any Error)?, cancelTask: Bool) {
        lock.lock()
        guard !completed else {
            lock.unlock()
            return
        }
        completed = true
        let responseContinuation = self.responseContinuation
        let bodyBuffer = self.bodyBuffer
        let task = self.task
        let session = self.session
        let transferPermit = self.transferPermit
        self.responseContinuation = nil
        self.bodyBuffer = nil
        self.bodyStream = nil
        self.task = nil
        self.session = nil
        self.transferPermit = nil
        let responseDelivered = self.responseDelivered
        lock.unlock()

        if cancelTask { task?.cancel() }
        session?.finishTasksAndInvalidate()
        transferPermit?.release()
        bodyBuffer?.finish(error: error)
        if responseDelivered {
            return
        } else if let error {
            responseContinuation?.resume(throwing: error)
        } else {
            responseContinuation?.resume(throwing: AgentRuntimeError.invalidResponse)
        }
    }

    private func suspendResponseTaskIfNeeded() {
        lock.lock()
        guard !completed, !responseTaskSuspended, let task else {
            lock.unlock()
            return
        }
        responseTaskSuspended = true
        task.suspend()
        lock.unlock()
    }

    private func resumeResponseTaskIfNeeded() {
        lock.lock()
        guard !completed, responseTaskSuspended, let task else {
            lock.unlock()
            return
        }
        responseTaskSuspended = false
        task.resume()
        lock.unlock()
    }

    private func releaseTransferPermit() {
        lock.lock()
        let transferPermit = self.transferPermit
        self.transferPermit = nil
        lock.unlock()
        transferPermit?.release()
    }
}

private final class AgentFileUploadResponseBuffer: @unchecked Sendable {
    enum EnqueueResult {
        case accepted
        case suspend
        case overflow
    }

    private static let highWatermarkBytes = 256 * 1_024
    private static let lowWatermarkBytes = 64 * 1_024
    private static let maximumBufferedBytes = 2 * 1_024 * 1_024

    private let lock = NSLock()
    private let onConsumerCancellation: @Sendable () -> Void
    private let onLowWatermark: @Sendable () -> Void
    private var chunks: [Data] = []
    private var bufferedBytes = 0
    private var waiter: CheckedContinuation<Data?, any Error>?
    private var finished = false
    private var terminalError: (any Error)?

    init(
        onConsumerCancellation: @escaping @Sendable () -> Void,
        onLowWatermark: @escaping @Sendable () -> Void
    ) {
        self.onConsumerCancellation = onConsumerCancellation
        self.onLowWatermark = onLowWatermark
    }

    func makeStream() -> AsyncThrowingStream<Data, Error> {
        let lifetime = AgentFileUploadStreamLifetime(onDeinit: onConsumerCancellation)
        return AsyncThrowingStream(unfolding: { [self, lifetime] in
            _ = lifetime
            return try await next()
        })
    }

    func enqueue(_ data: Data) -> EnqueueResult {
        guard !data.isEmpty else { return .accepted }
        lock.lock()
        guard !finished else {
            lock.unlock()
            return .accepted
        }
        if let waiter {
            self.waiter = nil
            lock.unlock()
            waiter.resume(returning: data)
            return .accepted
        }
        guard data.count <= Self.maximumBufferedBytes - bufferedBytes else {
            lock.unlock()
            return .overflow
        }
        chunks.append(data)
        bufferedBytes += data.count
        let shouldSuspend = bufferedBytes >= Self.highWatermarkBytes
        lock.unlock()
        return shouldSuspend ? .suspend : .accepted
    }

    func finish(error: (any Error)?) {
        lock.lock()
        guard !finished else {
            lock.unlock()
            return
        }
        finished = true
        terminalError = error
        let waiter = self.waiter
        self.waiter = nil
        lock.unlock()
        guard let waiter else { return }
        if let error {
            waiter.resume(throwing: error)
        } else {
            waiter.resume(returning: nil)
        }
    }

    private func next() async throws -> Data? {
        try Task.checkCancellation()
        return try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                lock.lock()
                if !chunks.isEmpty {
                    let chunk = chunks.removeFirst()
                    bufferedBytes -= chunk.count
                    let shouldResume = bufferedBytes <= Self.lowWatermarkBytes
                    lock.unlock()
                    if shouldResume { onLowWatermark() }
                    continuation.resume(returning: chunk)
                    return
                }
                if finished {
                    let error = terminalError
                    lock.unlock()
                    if let error {
                        continuation.resume(throwing: error)
                    } else {
                        continuation.resume(returning: nil)
                    }
                    return
                }
                guard waiter == nil else {
                    lock.unlock()
                    continuation.resume(throwing: AgentRuntimeError.invalidResponse)
                    return
                }
                waiter = continuation
                lock.unlock()
            }
        } onCancel: {
            onConsumerCancellation()
        }
    }
}

private final class AgentFileUploadStreamLifetime: @unchecked Sendable {
    private let onDeinit: @Sendable () -> Void

    init(onDeinit: @escaping @Sendable () -> Void) {
        self.onDeinit = onDeinit
    }

    deinit { onDeinit() }
}
