import Foundation

public enum BoundedURLSessionDataLoaderError: Error, Sendable {
    case invalidResponse
    case responseTooLarge
}

/// Incrementally receives one HTTP response and cancels its data task as soon
/// as the configured byte limit is exceeded. The delegate owns a private
/// session so cancellation and completion cannot race a shared session's state.
public enum BoundedURLSessionDataLoader {
    public static func load(
        request: URLRequest,
        session: URLSession = .shared,
        maximumBytes: Int
    ) async throws -> (data: Data, response: URLResponse) {
        guard maximumBytes > 0 else {
            throw BoundedURLSessionDataLoaderError.invalidResponse
        }
        let loader = Loader(maximumBytes: maximumBytes)
        return try await loader.load(
            request: request,
            configuration: session.configuration
        )
    }
}

private final class Loader: NSObject, URLSessionDataDelegate, @unchecked Sendable {
    private let lock = NSLock()
    private let maximumBytes: Int
    private var data = Data()
    private var response: URLResponse?
    private var continuation: CheckedContinuation<(Data, URLResponse), any Error>?
    private var session: URLSession?
    private var task: URLSessionDataTask?
    private var completed = false

    init(maximumBytes: Int) {
        self.maximumBytes = maximumBytes
    }

    func load(
        request: URLRequest,
        configuration: URLSessionConfiguration
    ) async throws -> (Data, URLResponse) {
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                start(
                    request: request,
                    configuration: configuration,
                    continuation: continuation
                )
            }
        } onCancel: {
            self.cancel()
        }
    }

    private func start(
        request: URLRequest,
        configuration: URLSessionConfiguration,
        continuation: CheckedContinuation<(Data, URLResponse), any Error>
    ) {
        lock.lock()
        guard !completed else {
            lock.unlock()
            continuation.resume(throwing: CancellationError())
            return
        }
        self.continuation = continuation
        let session = URLSession(
            configuration: configuration,
            delegate: self,
            delegateQueue: nil
        )
        let task = session.dataTask(with: request)
        self.session = session
        self.task = task
        lock.unlock()
        task.resume()
    }

    private func cancel() {
        finish(.failure(CancellationError()), cancelTask: true)
    }

    func urlSession(
        _ session: URLSession,
        dataTask: URLSessionDataTask,
        didReceive response: URLResponse,
        completionHandler: @escaping @Sendable (URLSession.ResponseDisposition) -> Void
    ) {
        let expectedLength = response.expectedContentLength
        guard expectedLength <= 0 || expectedLength <= Int64(maximumBytes) else {
            completionHandler(.cancel)
            finish(
                .failure(BoundedURLSessionDataLoaderError.responseTooLarge),
                cancelTask: true
            )
            return
        }
        lock.lock()
        if !completed { self.response = response }
        let shouldContinue = !completed
        lock.unlock()
        completionHandler(shouldContinue ? .allow : .cancel)
    }

    func urlSession(
        _ session: URLSession,
        dataTask: URLSessionDataTask,
        didReceive chunk: Data
    ) {
        lock.lock()
        guard !completed else {
            lock.unlock()
            return
        }
        guard chunk.count <= maximumBytes,
              data.count <= maximumBytes - chunk.count else {
            lock.unlock()
            finish(
                .failure(BoundedURLSessionDataLoaderError.responseTooLarge),
                cancelTask: true
            )
            return
        }
        data.append(chunk)
        lock.unlock()
    }

    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didCompleteWithError error: (any Error)?
    ) {
        if let error {
            finish(.failure(error), cancelTask: false)
            return
        }
        lock.lock()
        let result = response.map { (data, $0) }
        lock.unlock()
        guard let result else {
            finish(
                .failure(BoundedURLSessionDataLoaderError.invalidResponse),
                cancelTask: false
            )
            return
        }
        finish(.success(result), cancelTask: false)
    }

    private func finish(
        _ result: Result<(Data, URLResponse), any Error>,
        cancelTask: Bool
    ) {
        lock.lock()
        guard !completed else {
            lock.unlock()
            return
        }
        completed = true
        let continuation = self.continuation
        let task = self.task
        let session = self.session
        self.continuation = nil
        self.task = nil
        self.session = nil
        lock.unlock()
        if cancelTask { task?.cancel() }
        session?.finishTasksAndInvalidate()
        continuation?.resume(with: result)
    }
}
