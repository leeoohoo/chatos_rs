import Foundation
import Testing
import ChatOSCore
@testable import ChatOSConnector

@Suite(.serialized)
struct NativeLocalAgentExternalMCPExecutorTests {
    @Test
    func selectedRouteUsesJSONRPCToolsCallWithoutPersistingHeadersInSchema() async throws {
        let host = ExternalMCPHostStub(selectedIDs: ["mcp-1"])
        let session = URLSession(configuration: Self.sessionConfiguration())
        let executor = NativeLocalAgentExternalMCPExecutor(host: host, session: session)
        let config = Self.config()
        try await executor.configure([config])
        ExternalMCPURLProtocol.handler = { request in
            #expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer secret")
            let body = try Self.requestBody(request)
            let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: body)
            let object = try #require(Self.object(value))
            #expect(object["method"] == .string("tools/call"))
            #expect(Self.object(object["params"])?["name"] == .string("search"))
            return Self.response(
                request,
                body: #"{"jsonrpc":"2.0","id":"call-1","result":{"content":"ok"}}"#
            )
        }

        let output = try await executor.execute(
            ownerUserID: "user-1",
            invocation: Self.invocation()
        )
        #expect(Self.object(output)?["content"] == .string("ok"))

        let published = NativeLocalAgentPlatformToolCatalog.taskExecutionCapabilityTools(
            externalMCPConfigs: [config]
        ).first { Self.object($0)?["name"] == .string(config.tools[0].publicName) }
        let publishedData = try JSONEncoder().encode(try #require(published))
        let serialized = try #require(String(data: publishedData, encoding: .utf8))
        #expect(!serialized.contains("Bearer secret"))
        #expect(!serialized.contains("Authorization"))
    }

    @Test
    func rejectsRouteThatWasNotFrozenIntoTheTask() async throws {
        let executor = NativeLocalAgentExternalMCPExecutor(
            host: ExternalMCPHostStub(selectedIDs: []),
            session: URLSession(configuration: Self.sessionConfiguration())
        )
        try await executor.configure([Self.config()])
        ExternalMCPURLProtocol.handler = { request in
            Issue.record("Unselected external MCP must not make a request: \(request)")
            return Self.response(request, body: #"{"result":{}}"#)
        }

        await #expect(throws: Error.self) {
            try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation())
        }
    }

    @Test
    func rejectsOversizedAndJSONRPCErrorResponses() async throws {
        let executor = NativeLocalAgentExternalMCPExecutor(
            host: ExternalMCPHostStub(selectedIDs: ["mcp-1"]),
            session: URLSession(configuration: Self.sessionConfiguration())
        )
        try await executor.configure([Self.config()])
        ExternalMCPURLProtocol.handler = { request in
            let response = HTTPURLResponse(
                url: try #require(request.url),
                statusCode: 200,
                httpVersion: "HTTP/1.1",
                headerFields: ["Content-Length": String(17 * 1_024 * 1_024)]
            )!
            return (response, Data(#"{"result":{}}"#.utf8))
        }
        await #expect(throws: Error.self) {
            try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation())
        }

        ExternalMCPURLProtocol.handler = { request in
            Self.response(request, body: #"{"jsonrpc":"2.0","id":"call-1","error":{"code":-1,"message":"failed"}}"#)
        }
        await #expect(throws: Error.self) {
            try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation())
        }
    }

    private static func config() -> NativeLocalAgentExternalMCPConfig {
        .init(
            resourceID: "mcp-1",
            serverName: "catalog",
            url: URL(string: "https://mcp.example.test/rpc")!,
            headers: ["Authorization": "Bearer secret"],
            tools: [.init(
                publicName: "external_mcp__catalog__search",
                upstreamName: "search",
                description: "Search",
                inputSchema: .object([
                    "type": .string("object"),
                    "additionalProperties": .bool(false),
                ])
            )]
        )
    }

    private static func invocation() -> LocalAgentToolInvocationRecord {
        .init(
            invocationID: "invocation-1",
            runID: "run-1",
            batchID: "batch-1",
            callID: "call-1",
            toolName: "external_mcp__catalog__search",
            arguments: .object(["query": .string("test")]),
            sideEffecting: true,
            requiresApproval: false,
            approvalStatus: "not_required",
            approvalDecidedBy: nil,
            approvalReason: nil,
            approvalDecidedAtUnixMs: nil,
            status: "running",
            result: nil,
            error: nil,
            version: 2,
            claimToken: "claim-1",
            claimUntilUnixMs: 60_000,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
    }

    private static func sessionConfiguration() -> URLSessionConfiguration {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [ExternalMCPURLProtocol.self]
        return configuration
    }

    private static func object(
        _ value: LocalAgentJSONValue?
    ) -> [String: LocalAgentJSONValue]? {
        guard case let .object(object)? = value else { return nil }
        return object
    }

    private static func response(
        _ request: URLRequest,
        body: String
    ) -> (HTTPURLResponse, Data) {
        let response = HTTPURLResponse(
            url: request.url!,
            statusCode: 200,
            httpVersion: "HTTP/1.1",
            headerFields: ["Content-Type": "application/json"]
        )!
        return (response, Data(body.utf8))
    }

    private static func requestBody(_ request: URLRequest) throws -> Data {
        if let body = request.httpBody { return body }
        guard let stream = request.httpBodyStream else {
            throw CocoaError(.fileReadUnknown)
        }
        stream.open()
        defer { stream.close() }
        var body = Data()
        var buffer = [UInt8](repeating: 0, count: 4_096)
        while true {
            let count = stream.read(&buffer, maxLength: buffer.count)
            if count < 0 {
                throw stream.streamError ?? CocoaError(.fileReadUnknown)
            }
            if count == 0 { return body }
            body.append(contentsOf: buffer.prefix(count))
        }
    }
}

private actor ExternalMCPHostStub: LocalAgentHostClientServicing {
    private let selectedIDs: [String]

    init(selectedIDs: [String]) { self.selectedIDs = selectedIDs }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: command)
        guard case let .object(object) = value,
              object["type"] == .string("get_run") else {
            throw CocoaError(.featureUnsupported)
        }
        return try JSONSerialization.data(withJSONObject: [
            "type": "run",
            "run": [
                "run_id": "run-1",
                "owner_user_id": "user-1",
                "owner_entity_type": "task",
                "owner_entity_id": "task-1",
                "profile_key": "task_execution",
                "input": [
                    "tool_options": ["external_mcp_config_ids": selectedIDs],
                ],
                "status": "model_running",
                "version": 2,
                "terminal_outcome": NSNull(),
                "created_at_unix_ms": 1,
                "updated_at_unix_ms": 1,
            ],
        ])
    }
}

private final class ExternalMCPURLProtocol: URLProtocol, @unchecked Sendable {
    nonisolated(unsafe) static var handler: ((URLRequest) throws -> (HTTPURLResponse, Data))?

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        do {
            guard let handler = Self.handler else { throw CocoaError(.featureUnsupported) }
            let (response, data) = try handler(request)
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: data)
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }

    override func stopLoading() {}
}
