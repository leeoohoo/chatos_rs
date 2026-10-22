@testable import ChatOSAPI
import ChatOSAgentRuntime
import Darwin
import Foundation
import XCTest

final class HTTPTransportTests: XCTestCase {
    func testBoundedResponseAcceptsBodyWithinLimit() async throws {
        BoundedResponseURLProtocol.setBody(Data(repeating: 0x61, count: 512))
        let response = try await transport().send(.init(
            url: URL(string: "https://bounded-response.test/ok")!,
            method: "GET",
            maximumResponseBytes: 1_024
        ))

        XCTAssertEqual(response.statusCode, 200)
        XCTAssertEqual(response.body.count, 512)
    }

    func testBoundedResponseCancelsBeforeRetainingOversizedBody() async throws {
        BoundedResponseURLProtocol.setBody(Data(repeating: 0x62, count: 2_048))

        do {
            _ = try await transport().send(.init(
                url: URL(string: "https://bounded-response.test/oversized")!,
                method: "GET",
                maximumResponseBytes: 1_024
            ))
            XCTFail("Oversized response unexpectedly succeeded")
        } catch ChatOSAPIError.invalidResponse {
            // Expected: the delegate cancels as soon as the configured limit is crossed.
        }
    }

    func testFileBackedUploadKeepsResponseBounded() async throws {
        let bodyURL = FileManager.default.temporaryDirectory.appendingPathComponent(
            "http-upload-\(UUID().uuidString).json"
        )
        let uploadBody = Data(#"{"payload":"from-file"}"#.utf8)
        try uploadBody.write(to: bodyURL, options: .atomic)
        defer { try? FileManager.default.removeItem(at: bodyURL) }

        BoundedResponseURLProtocol.setBody(Data(repeating: 0x63, count: 512))
        let response = try await transport().sendFileBacked(.init(
            url: URL(string: "https://bounded-response.test/upload")!,
            method: "POST",
            headers: ["Content-Type": "application/json"],
            bodyFile: .init(url: bodyURL, byteCount: uploadBody.count),
            maximumResponseBytes: 1_024
        ))
        XCTAssertEqual(response.statusCode, 200)
        XCTAssertEqual(response.body.count, 512)

        BoundedResponseURLProtocol.setBody(Data(repeating: 0x64, count: 2_048))
        do {
            _ = try await transport().sendFileBacked(.init(
                url: URL(string: "https://bounded-response.test/upload-oversized")!,
                method: "POST",
                bodyFile: .init(url: bodyURL, byteCount: uploadBody.count),
                maximumResponseBytes: 1_024
            ))
            XCTFail("Oversized upload response unexpectedly succeeded")
        } catch ChatOSAPIError.invalidResponse {
            // Expected: upload and response buffering share the same response cap.
        }
    }

    func testResponseBodyFileWriterStreamsAndRemovesOversizedPartialFile() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "response-writer-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let expected = Data((0..<150_000).map { UInt8($0 % 251) })
        let stream = AsyncThrowingStream<Data, Error> { continuation in
            continuation.yield(expected.prefix(75_000))
            continuation.yield(expected.suffix(75_000))
            continuation.finish()
        }
        let file = try await ChatOSResponseBodyFileWriter.write(
            stream,
            maximumBytes: expected.count,
            temporaryDirectory: root
        )
        let fileURL = file.url
        let artifactURL = fileURL.deletingLastPathComponent()
        XCTAssertEqual(file.byteCount, expected.count)
        XCTAssertEqual(try Data(contentsOf: fileURL), expected)
        XCTAssertTrue(artifactURL.lastPathComponent.hasPrefix(".chatos-video-download-"))
        XCTAssertEqual(
            Set(try FileManager.default.contentsOfDirectory(atPath: artifactURL.path)),
            [".lease", "payload.tmp"]
        )
        XCTAssertEqual(fileMode(artifactURL), 0o700)
        XCTAssertEqual(fileMode(artifactURL.appendingPathComponent(".lease")), 0o600)
        XCTAssertEqual(fileMode(fileURL), 0o600)
        file.removeIfNeeded()
        XCTAssertFalse(FileManager.default.fileExists(atPath: artifactURL.path))

        let oversized = AsyncThrowingStream<Data, Error> { continuation in
            continuation.yield(Data(repeating: 0x61, count: 6))
            continuation.yield(Data(repeating: 0x62, count: 6))
            continuation.finish()
        }
        do {
            _ = try await ChatOSResponseBodyFileWriter.write(
                oversized,
                maximumBytes: 10,
                temporaryDirectory: root
            )
            XCTFail("Oversized streamed body unexpectedly succeeded")
        } catch {
            XCTAssertEqual(
                try FileManager.default.contentsOfDirectory(atPath: root.path),
                []
            )
        }
    }

    func testResponseBodyFileWriterReconcilesOnlyReleasedExpiredLease() throws {
        let root = try makeTemporaryDirectory(prefix: "response-writer-lease")
        defer { try? FileManager.default.removeItem(at: root) }
        let artifact = try makeDownloadArtifact(in: root)
        try makeExpired(artifact.directory)

        let activeLease = Darwin.open(artifact.lease.path, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
        XCTAssertGreaterThanOrEqual(activeLease, 0)
        XCTAssertEqual(flock(activeLease, LOCK_EX | LOCK_NB), 0)

        XCTAssertEqual(
            ChatOSResponseBodyFileWriter.reconcileOrphanedArtifacts(
                in: root, olderThan: Date(), maximumCandidates: 64
            ),
            0
        )
        XCTAssertTrue(FileManager.default.fileExists(atPath: artifact.directory.path))

        _ = flock(activeLease, LOCK_UN)
        _ = Darwin.close(activeLease)
        XCTAssertEqual(
            ChatOSResponseBodyFileWriter.reconcileOrphanedArtifacts(
                in: root, olderThan: Date(), maximumCandidates: 64
            ),
            1
        )
        XCTAssertFalse(FileManager.default.fileExists(atPath: artifact.directory.path))
    }

    func testResponseBodyFileWriterPreservesFreshAndMalformedArtifacts() throws {
        let root = try makeTemporaryDirectory(prefix: "response-writer-safety")
        defer { try? FileManager.default.removeItem(at: root) }
        let fresh = try makeDownloadArtifact(in: root)

        let malformed = root.appendingPathComponent(
            ".chatos-video-download-not-a-uuid", isDirectory: true
        )
        try FileManager.default.createDirectory(at: malformed, withIntermediateDirectories: false)
        try makeExpired(malformed)

        let symlinkTarget = try makeTemporaryDirectory(prefix: "response-writer-symlink-target")
        defer { try? FileManager.default.removeItem(at: symlinkTarget) }
        let symlinkDirectory = artifactDirectory(in: root)
        try FileManager.default.createSymbolicLink(at: symlinkDirectory, withDestinationURL: symlinkTarget)

        let symlinkPayload = try makeDownloadArtifact(in: root, payload: false)
        let payloadTarget = root.appendingPathComponent("payload-target")
        try Data("target".utf8).write(to: payloadTarget)
        try FileManager.default.createSymbolicLink(
            at: symlinkPayload.payload, withDestinationURL: payloadTarget
        )
        try makeExpired(symlinkPayload.directory)

        let hardLinkedPayload = try makeDownloadArtifact(in: root, payload: false)
        let hardLinkTarget = root.appendingPathComponent("hard-link-target")
        try Data("target".utf8).write(to: hardLinkTarget)
        try FileManager.default.linkItem(at: hardLinkTarget, to: hardLinkedPayload.payload)
        try makeExpired(hardLinkedPayload.directory)

        let unknownFile = try makeDownloadArtifact(in: root)
        try Data("unknown".utf8).write(
            to: unknownFile.directory.appendingPathComponent("unexpected")
        )
        try makeExpired(unknownFile.directory)

        XCTAssertEqual(
            ChatOSResponseBodyFileWriter.reconcileOrphanedArtifacts(
                in: root,
                olderThan: Date().addingTimeInterval(-60),
                maximumCandidates: 64
            ),
            0
        )
        for preserved in [
            fresh.directory, malformed, symlinkDirectory, symlinkPayload.directory,
            hardLinkedPayload.directory, unknownFile.directory,
        ] {
            XCTAssertTrue(
                FileManager.default.fileExists(atPath: preserved.path),
                "Expected \(preserved.lastPathComponent) to be preserved"
            )
        }
    }

    func testResponseBodyFileWriterBoundsOrphanReconciliation() throws {
        let root = try makeTemporaryDirectory(prefix: "response-writer-limit")
        defer { try? FileManager.default.removeItem(at: root) }
        for _ in 0..<65 {
            let artifact = try makeDownloadArtifact(in: root)
            try makeExpired(artifact.directory)
        }

        XCTAssertEqual(
            ChatOSResponseBodyFileWriter.reconcileOrphanedArtifacts(
                in: root, olderThan: Date(), maximumCandidates: 64
            ),
            64
        )
        XCTAssertEqual(try downloadArtifactDirectories(in: root).count, 1)
        XCTAssertEqual(
            ChatOSResponseBodyFileWriter.reconcileOrphanedArtifacts(
                in: root, olderThan: Date(), maximumCandidates: 64
            ),
            1
        )
        XCTAssertEqual(try downloadArtifactDirectories(in: root).count, 0)
    }

    func testResponseBodyFileWriterRemovesCancelledAndFailedArtifacts() async throws {
        let root = try makeTemporaryDirectory(prefix: "response-writer-cancel")
        defer { try? FileManager.default.removeItem(at: root) }
        var continuation: AsyncThrowingStream<Data, Error>.Continuation!
        let stream = AsyncThrowingStream<Data, Error> { continuation = $0 }
        let writing = Task {
            try await ChatOSResponseBodyFileWriter.write(
                stream, maximumBytes: 64, temporaryDirectory: root
            )
        }
        continuation.yield(Data("partial".utf8))
        for _ in 0..<100 where (try? downloadArtifactDirectories(in: root).isEmpty) == true {
            try await Task.sleep(for: .milliseconds(5))
        }
        writing.cancel()
        do {
            _ = try await writing.value
            XCTFail("Cancelled writer unexpectedly succeeded")
        } catch is CancellationError {
            // Expected.
        }
        XCTAssertTrue(try downloadArtifactDirectories(in: root).isEmpty)

        enum StreamFailure: Error { case failed }
        let failed = AsyncThrowingStream<Data, Error> { continuation in
            continuation.yield(Data("partial".utf8))
            continuation.finish(throwing: StreamFailure.failed)
        }
        do {
            _ = try await ChatOSResponseBodyFileWriter.write(
                failed, maximumBytes: 64, temporaryDirectory: root
            )
            XCTFail("Failed stream unexpectedly succeeded")
        } catch MediaGenerationClientError.invalidVideoContent {
            XCTAssertTrue(try downloadArtifactDirectories(in: root).isEmpty)
        }
    }

    func testResponseBodyFileWriterRejectsLowDiskBeforeCreatingOrConsumingFile() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "response-writer-low-disk-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let stream = AsyncThrowingStream<Data, Error> { continuation in
            continuation.yield(Data("video".utf8))
            continuation.finish()
        }

        do {
            _ = try await ChatOSResponseBodyFileWriter.write(
                stream,
                maximumBytes: 64,
                expectedBytes: 5,
                temporaryDirectory: root,
                availableCapacityOverride: 0
            )
            XCTFail("Low disk capacity unexpectedly admitted a media download")
        } catch MediaGenerationClientError.insufficientDiskSpace {
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), [])
        }

        let truncated = AsyncThrowingStream<Data, Error> { continuation in
            continuation.yield(Data("four".utf8))
            continuation.finish()
        }
        do {
            _ = try await ChatOSResponseBodyFileWriter.write(
                truncated,
                maximumBytes: 64,
                expectedBytes: 5,
                temporaryDirectory: root,
                availableCapacityOverride: Int64.max
            )
            XCTFail("A truncated Content-Length body unexpectedly succeeded")
        } catch MediaGenerationClientError.invalidVideoContent {
            XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: root.path), [])
        }
    }

    func testBurstStreamSurvivesDelayedAndSlowConsumerWithoutLosingBytes() async throws {
        // Far more than the former 64-chunk queue; each SSE line is a chunk.
        let payload = Data((0..<512).map { "data: 第\($0)条\n\n" }.joined().utf8)
        BoundedResponseURLProtocol.setBody(payload)
        let response = try await transport().stream(.init(
            url: URL(string: "https://bounded-response.test/burst")!, method: "GET"
        ))
        // Previously the eager producer filled its queue and failed before we read it.
        try await Task.sleep(for: .milliseconds(50))
        var received = Data()
        var chunks = 0
        for try await chunk in response.body {
            XCTAssertLessThanOrEqual(chunk.count, 4_096)
            received.append(chunk)
            chunks += 1
            if chunks.isMultiple(of: 16) { try await Task.sleep(for: .milliseconds(1)) }
        }
        XCTAssertGreaterThan(chunks, 64)
        XCTAssertEqual(received, payload)
    }

    func testStreamPreservesLongLinesAndFinalUnterminatedChunk() async throws {
        let payload = Data((String(repeating: "界", count: 4_000) + "\n末尾无换行").utf8)
        BoundedResponseURLProtocol.setBody(payload)
        let response = try await transport().stream(.init(
            url: URL(string: "https://bounded-response.test/long-line")!, method: "GET"
        ))
        var chunks: [Data] = []
        for try await chunk in response.body {
            XCTAssertFalse(chunk.isEmpty)
            XCTAssertLessThanOrEqual(chunk.count, 4_096)
            chunks.append(chunk)
        }
        XCTAssertEqual(chunks.reduce(into: Data()) { $0.append($1) }, payload)
        XCTAssertEqual(String(data: chunks.last!, encoding: .utf8), "末尾无换行")
    }

    func testAgentResponsesCompletesWithSlowEventHandling() async throws {
        let text = String(repeating: "字", count: 256)
        let completion: [String: Any] = [
            "type": "response.completed",
            "response": ["id": "resp_burst", "status": "completed", "output": [
                ["type": "message", "role": "assistant", "content": [
                    ["type": "output_text", "text": text],
                ]],
            ]],
        ]
        let delta = "data: {\"type\":\"response.output_text.delta\",\"delta\":\"字\"}\n\n"
        var payload = Data(String(repeating: delta, count: 256).utf8)
        payload.append(Data("data: ".utf8))
        payload.append(try JSONSerialization.data(withJSONObject: completion))
        payload.append(Data("\n\n".utf8))
        BoundedResponseURLProtocol.setBody(payload, contentType: "text/event-stream")
        let transport = transport()
        let model = try AgentResponsesModelClient(
            baseURL: URL(string: "https://bounded-response.test/v1")!,
            model: "test", apiKey: "test-key",
            streamTransport: { request in
                let response = try await transport.stream(.init(
                    url: request.url!, method: "POST", body: request.httpBody
                ))
                return .init(statusCode: response.statusCode, headers: response.headers, body: response.body)
            }
        )
        let recorder = StreamEventRecorder()
        let result = try await model.stream(
            messages: [.init(role: .user, content: "test")], tools: [], timeout: 30,
            onEvent: { event in
                await recorder.record(event)
                try? await Task.sleep(for: .milliseconds(1))
            }
        )
        let recorded = await recorder.snapshot()
        XCTAssertEqual(result.content, text)
        XCTAssertEqual(recorded.text, text)
        XCTAssertTrue(recorded.completed)
    }

    func testCancellingStreamConsumerCancelsUnderlyingRequest() async throws {
        let stopped = expectation(description: "URL request cancelled")
        let firstChunk = expectation(description: "consumer started")
        BoundedResponseURLProtocol.setBody(Data("data: first\n".utf8), finishes: false, stopped: stopped)
        let response = try await transport().stream(.init(
            url: URL(string: "https://bounded-response.test/cancel")!, method: "GET"
        ))
        let consuming = Task {
            for try await _ in response.body { firstChunk.fulfill() }
        }
        await fulfillment(of: [firstChunk], timeout: 2)
        consuming.cancel()
        do {
            try await consuming.value
            XCTFail("Cancelled stream unexpectedly finished successfully")
        } catch {
            XCTAssertTrue(error is CancellationError || (error as? URLError)?.code == .cancelled)
        }
        await fulfillment(of: [stopped], timeout: 2)
    }

    private func transport() -> URLSessionHTTPTransport {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [BoundedResponseURLProtocol.self]
        return URLSessionHTTPTransport(session: URLSession(configuration: configuration))
    }

    private func makeTemporaryDirectory(prefix: String) throws -> URL {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "\(prefix)-\(UUID().uuidString)", isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        return root
    }

    private func artifactDirectory(in root: URL) -> URL {
        root.appendingPathComponent(
            ".chatos-video-download-\(UUID().uuidString.lowercased())", isDirectory: true
        )
    }

    private func makeDownloadArtifact(
        in root: URL,
        payload: Bool = true
    ) throws -> (directory: URL, lease: URL, payload: URL) {
        let directory = artifactDirectory(in: root)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        let lease = directory.appendingPathComponent(".lease")
        let payloadURL = directory.appendingPathComponent("payload.tmp")
        XCTAssertTrue(FileManager.default.createFile(atPath: lease.path, contents: Data()))
        if payload {
            XCTAssertTrue(FileManager.default.createFile(atPath: payloadURL.path, contents: Data("video".utf8)))
        }
        return (directory, lease, payloadURL)
    }

    private func makeExpired(_ url: URL) throws {
        try FileManager.default.setAttributes(
            [.modificationDate: Date().addingTimeInterval(-48 * 60 * 60)],
            ofItemAtPath: url.path
        )
    }

    private func downloadArtifactDirectories(in root: URL) throws -> [URL] {
        try FileManager.default.contentsOfDirectory(
            at: root, includingPropertiesForKeys: nil
        ).filter { $0.lastPathComponent.hasPrefix(".chatos-video-download-") }
    }

    private func fileMode(_ url: URL) -> mode_t? {
        var status = stat()
        let result = url.withUnsafeFileSystemRepresentation { path in
            path.map { Darwin.lstat($0, &status) } ?? -1
        }
        return result == 0 ? status.st_mode & mode_t(0o777) : nil
    }

}

private final class BoundedResponseURLProtocol: URLProtocol, @unchecked Sendable {
    private static let lock = NSLock()
    nonisolated(unsafe) private static var body = Data()
    nonisolated(unsafe) private static var finishes = true
    nonisolated(unsafe) private static var contentType = "application/octet-stream"
    nonisolated(unsafe) private static var stopped: XCTestExpectation?
    private var stopExpectation: XCTestExpectation?

    static func setBody(
        _ value: Data, finishes: Bool = true, stopped: XCTestExpectation? = nil,
        contentType: String = "application/octet-stream"
    ) {
        lock.lock()
        body = value
        Self.finishes = finishes
        Self.stopped = stopped
        Self.contentType = contentType
        lock.unlock()
    }

    override class func canInit(with request: URLRequest) -> Bool {
        request.url?.host == "bounded-response.test"
    }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest {
        request
    }

    override func startLoading() {
        Self.lock.lock()
        let body = Self.body
        let finishes = Self.finishes
        let contentType = Self.contentType
        stopExpectation = Self.stopped
        Self.lock.unlock()
        guard let url = request.url,
              let response = HTTPURLResponse(
                url: url,
                statusCode: 200,
                httpVersion: "HTTP/1.1",
                headerFields: ["Content-Type": contentType]
              ) else {
            client?.urlProtocol(self, didFailWithError: ChatOSAPIError.invalidResponse)
            return
        }
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        for chunkStart in stride(from: 0, to: body.count, by: 256) {
            let chunkEnd = min(body.count, chunkStart + 256)
            client?.urlProtocol(self, didLoad: body.subdata(in: chunkStart..<chunkEnd))
        }
        if finishes { client?.urlProtocolDidFinishLoading(self) }
    }

    override func stopLoading() { stopExpectation?.fulfill() }
}

private actor StreamEventRecorder {
    private var text = ""
    private var completed = false

    func record(_ event: AgentModelStreamEvent) {
        switch event {
        case let .textDelta(delta): text += delta
        case .completed: completed = true
        default: break
        }
    }

    func snapshot() -> (text: String, completed: Bool) { (text, completed) }
}
