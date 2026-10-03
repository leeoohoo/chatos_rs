@testable import ChatOSAPI
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

    private func transport() -> URLSessionHTTPTransport {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [BoundedResponseURLProtocol.self]
        return URLSessionHTTPTransport(session: URLSession(configuration: configuration))
    }
}

private final class BoundedResponseURLProtocol: URLProtocol, @unchecked Sendable {
    private static let lock = NSLock()
    nonisolated(unsafe) private static var body = Data()

    static func setBody(_ value: Data) {
        lock.lock()
        body = value
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
        Self.lock.unlock()
        guard let url = request.url,
              let response = HTTPURLResponse(
                url: url,
                statusCode: 200,
                httpVersion: "HTTP/1.1",
                headerFields: ["Content-Type": "application/octet-stream"]
              ) else {
            client?.urlProtocol(self, didFailWithError: ChatOSAPIError.invalidResponse)
            return
        }
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        for chunkStart in stride(from: 0, to: body.count, by: 256) {
            let chunkEnd = min(body.count, chunkStart + 256)
            client?.urlProtocol(self, didLoad: body.subdata(in: chunkStart..<chunkEnd))
        }
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}
