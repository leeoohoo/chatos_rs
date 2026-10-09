@testable import ChatOSAPI
import ChatOSAgentRuntime
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
