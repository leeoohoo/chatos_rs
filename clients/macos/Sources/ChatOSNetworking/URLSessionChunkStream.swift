import Foundation

/// Pull chunks only when the consumer asks for them. A push-based bounded
/// AsyncThrowingStream drops bytes when SSE arrives faster than parsing/UI work,
/// corrupting an otherwise valid response. Keep chunks bounded without a lossy queue.
public enum URLSessionChunkStream {
    public static func body(
        from bytes: URLSession.AsyncBytes,
        transferPermit: ClientLargeTransferPermit? = nil
    ) -> AsyncThrowingStream<Data, Error> {
        let reader = Reader(bytes: bytes, transferPermit: transferPermit)
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
    private var transferPermit: ClientLargeTransferPermit?

    init(
        bytes: URLSession.AsyncBytes,
        transferPermit: ClientLargeTransferPermit?
    ) {
        iterator = bytes.makeAsyncIterator()
        task = bytes.task
        self.transferPermit = transferPermit
    }

    deinit {
        task.cancel()
        transferPermit?.release()
    }

    func next() async throws -> Data? {
        guard !Task.isCancelled else {
            task.cancel()
            await releaseTransferPermit()
            throw CancellationError()
        }
        guard var current = iterator else {
            await releaseTransferPermit()
            return nil
        }
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
            if chunk.isEmpty {
                await releaseTransferPermit()
                return nil
            }
            return chunk
        } catch {
            task.cancel()
            await releaseTransferPermit()
            throw error
        }
    }

    private func releaseTransferPermit() async {
        let permit = transferPermit
        transferPermit = nil
        await permit?.releaseAndWait()
    }
}
