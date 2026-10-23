import Foundation
import XCTest
@testable import ChatOSAgentRuntime

final class AgentURLSessionFileUploadTests: XCTestCase {
    func testFileUploadStreamsResponseWithoutMaterializingRequestBody() async throws {
        let bodyURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-file-upload-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: bodyURL) }
        let requestBody = Data(repeating: 0x61, count: 512 * 1_024 + 1)
        try requestBody.write(to: bodyURL)
        let responseBody = Data(repeating: 0x64, count: 512 * 1_024 + 1)
        AgentFileUploadURLProtocol.configure(responseBody: responseBody)

        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [AgentFileUploadURLProtocol.self]
        var request = URLRequest(url: URL(string: "https://agent-upload.test/stream")!)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue(String(requestBody.count), forHTTPHeaderField: "Content-Length")
        let response = try await AgentURLSessionStream.open(
            request,
            bodyFileURL: bodyURL,
            configuration: configuration
        )
        var received = Data()
        for try await chunk in response.body { received.append(chunk) }

        XCTAssertEqual(response.statusCode, 200)
        XCTAssertEqual(received, responseBody)
        XCTAssertEqual(AgentFileUploadURLProtocol.capturedBody(), requestBody)
    }

    func testCancellingResponseConsumptionCancelsFileUploadSession() async throws {
        let bodyURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("agent-file-upload-cancel-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: bodyURL) }
        try Data(repeating: 0x62, count: 128 * 1_024 + 1).write(to: bodyURL)
        let stopped = expectation(description: "file upload request cancelled")
        AgentFileUploadURLProtocol.configure(
            responseBody: Data("data: partial\n\n".utf8),
            finishes: false,
            stopped: stopped
        )

        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [AgentFileUploadURLProtocol.self]
        var request = URLRequest(url: URL(string: "https://agent-upload.test/cancel")!)
        request.httpMethod = "POST"
        let response = try await AgentURLSessionStream.open(
            request,
            bodyFileURL: bodyURL,
            configuration: configuration
        )
        let firstChunk = expectation(description: "response consumption started")
        let consuming = Task {
            for try await _ in response.body { firstChunk.fulfill() }
        }
        await fulfillment(of: [firstChunk], timeout: 2)
        consuming.cancel()
        do {
            try await consuming.value
            XCTFail("Cancelled response stream unexpectedly completed")
        } catch {
            XCTAssertTrue(
                error is CancellationError || (error as? URLError)?.code == .cancelled
            )
        }
        await fulfillment(of: [stopped], timeout: 2)
    }
}

private final class AgentFileUploadURLProtocol: URLProtocol, @unchecked Sendable {
    private static let lock = NSLock()
    nonisolated(unsafe) private static var responseBody = Data()
    nonisolated(unsafe) private static var uploadedBody = Data()
    nonisolated(unsafe) private static var finishes = true
    nonisolated(unsafe) private static var stopExpectation: XCTestExpectation?

    static func configure(
        responseBody: Data,
        finishes: Bool = true,
        stopped: XCTestExpectation? = nil
    ) {
        lock.lock()
        self.responseBody = responseBody
        uploadedBody = Data()
        self.finishes = finishes
        stopExpectation = stopped
        lock.unlock()
    }

    static func capturedBody() -> Data {
        lock.lock()
        defer { lock.unlock() }
        return uploadedBody
    }

    override class func canInit(with request: URLRequest) -> Bool {
        request.url?.host == "agent-upload.test"
    }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        do {
            let body: Data
            if let direct = request.httpBody {
                body = direct
            } else if let stream = request.httpBodyStream {
                stream.open()
                defer { stream.close() }
                var collected = Data()
                var buffer = [UInt8](repeating: 0, count: 64 * 1_024)
                while true {
                    let count = stream.read(&buffer, maxLength: buffer.count)
                    guard count >= 0 else { throw stream.streamError ?? URLError(.cannotDecodeRawData) }
                    if count == 0 { break }
                    collected.append(buffer, count: count)
                }
                body = collected
            } else {
                throw URLError(.cannotDecodeRawData)
            }
            Self.lock.lock()
            Self.uploadedBody = body
            let responseBody = Self.responseBody
            let finishes = Self.finishes
            Self.lock.unlock()
            guard let url = request.url,
                  let response = HTTPURLResponse(
                    url: url,
                    statusCode: 200,
                    httpVersion: "HTTP/1.1",
                    headerFields: ["Content-Type": "text/event-stream"]
                  ) else {
                throw URLError(.badServerResponse)
            }
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: responseBody)
            if finishes { client?.urlProtocolDidFinishLoading(self) }
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }

    override func stopLoading() {
        Self.lock.lock()
        let expectation = Self.stopExpectation
        Self.stopExpectation = nil
        Self.lock.unlock()
        expectation?.fulfill()
    }
}
