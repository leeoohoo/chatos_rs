import Foundation

/// Pull chunks only when the consumer asks for them. A push-based bounded
/// AsyncThrowingStream drops bytes when SSE arrives faster than parsing/UI work,
/// corrupting an otherwise valid response. Keep chunks bounded without a lossy queue.
public enum URLSessionChunkStream {
    public static func body(from bytes: URLSession.AsyncBytes) -> AsyncThrowingStream<Data, Error> {
        let reader = Reader(bytes: bytes)
        let task = bytes.task
        return AsyncThrowingStream(unfolding: {
            try await withTaskCancellationHandler {
                try await reader.next()
            } onCancel: {
                task.cancel()
            }
        })
    }
}

private actor Reader {
    private var iterator: URLSession.AsyncBytes.Iterator?
    private let task: URLSessionDataTask

    init(bytes: URLSession.AsyncBytes) {
        iterator = bytes.makeAsyncIterator()
        task = bytes.task
    }

    deinit { task.cancel() }

    func next() async throws -> Data? {
        try Task.checkCancellation()
        guard var current = iterator else { return nil }
        iterator = nil
        var chunk = Data()
        chunk.reserveCapacity(4_096)
        do {
            while let byte = try await current.next() {
                try Task.checkCancellation()
                chunk.append(byte)
                if byte == 10 || chunk.count == 4_096 {
                    iterator = current
                    return chunk
                }
            }
            return chunk.isEmpty ? nil : chunk
        } catch {
            task.cancel()
            throw error
        }
    }
}
