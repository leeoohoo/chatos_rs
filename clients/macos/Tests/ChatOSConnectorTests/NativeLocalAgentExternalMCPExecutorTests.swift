import Foundation
import Testing
import ChatOSCore
@testable import ChatOSConnector

@Suite(.serialized)
struct NativeLocalAgentExternalMCPExecutorTests {
    @Test
    func offlineMissingRuntimeIsKnownFailureWithoutHTTP() async throws {
        let executor = NativeLocalAgentExternalMCPExecutor(
            host: ExternalMCPHostStub(selectedIDs: ["mcp-1"]),
            session: URLSession(configuration: Self.sessionConfiguration())
        )
        ExternalMCPURLProtocol.handler = { request in
            Issue.record("Missing transient runtime must not reach HTTP")
            return Self.response(request, body: #"{"result":{}}"#)
        }
        let rejected = try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation())
        #expect(Self.object(rejected)?["is_error"] == .bool(true))
        #expect(Self.object(Self.object(rejected)?["content"])?["error"] == .string("external_mcp_runtime_unavailable"))
        let platform = NativeLocalAgentPlatformToolExecutor(
            host: ExternalMCPHostStub(selectedIDs: ["mcp-1"]),
            attachmentRootURL: FileManager.default.temporaryDirectory
        )
        let routed = try await platform.execute(ownerUserID: "user-1", invocation: Self.invocation())
        #expect(routed == rejected, "platform must route an unconfigured frozen tool to its known preflight failure")
    }

    @Test
    func frozenCatalogAndActivationAreCheckedBeforeHTTP() async throws {
        let config = Self.config()
        let bundle = try NativeLocalConnectorService.taskExecutionSkillSnapshot(externalMCPConfigs: [config]).bundle
        ExternalMCPURLProtocol.handler = { request in
            Issue.record("Rejected gate must not reach HTTP")
            return Self.response(request, body: #"{"result":{}}"#)
        }
        let inactive = NativeLocalAgentExternalMCPExecutor(
            host: ExternalMCPHostStub(selectedIDs: ["mcp-1"], bundle: bundle),
            session: URLSession(configuration: Self.sessionConfiguration())
        )
        try await inactive.configure([config])
        let blocked = try await inactive.execute(ownerUserID: "user-1", invocation: Self.invocation())
        #expect(Self.object(blocked)?["is_error"] == .bool(true))
        #expect(Self.object(Self.object(blocked)?["content"])?["error"] == .string("skill_activation_required"))

        let host = ExternalMCPHostStub(selectedIDs: ["mcp-1"], bundle: bundle,
            activatedRefs: ["product-skill:chatos-external-mcp"])
        let executor = NativeLocalAgentExternalMCPExecutor(host: host, session: URLSession(configuration: Self.sessionConfiguration()))
        let changed = NativeLocalAgentExternalMCPConfig(resourceID: config.resourceID, serverName: config.serverName,
            url: URL(string: "https://replacement.example.test/rpc")!, headers: config.headers, tools: config.tools)
        try await executor.configure([changed])
        let mismatch = try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation())
        #expect(Self.object(Self.object(mismatch)?["content"])?["error"] == .string("external_mcp_snapshot_changed"))

        let rotated = NativeLocalAgentExternalMCPConfig(resourceID: config.resourceID, serverName: config.serverName,
            url: config.url, headers: ["Authorization":"Bearer rotated"], tools: config.tools)
        #expect(try rotated.catalogSHA256() == config.catalogSHA256())
        ExternalMCPURLProtocol.handler = { request in
            #expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer rotated")
            return Self.response(request, body: #"{"result":{"content":"verified"}}"#)
        }
        try await executor.configure([rotated])
        let accepted = try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation())
        #expect(Self.object(accepted)?["content"] == .string("verified"))
    }

    @Test
    func remoteBusinessGateWithoutTrustedDocumentsFailsClosed() async throws {
        let config = Self.config()
        let tool = config.tools[0]
        let gated = NativeLocalAgentExternalMCPConfig(resourceID: config.resourceID, serverName: config.serverName,
            url: config.url, headers: config.headers, tools: [.init(publicName: tool.publicName,
                upstreamName: tool.upstreamName, description: tool.description, inputSchema: tool.inputSchema,
                remoteSkillGate: .object(["allOf": .array([.string("remote-business")])]))])
        let executor = NativeLocalAgentExternalMCPExecutor(host: ExternalMCPHostStub(selectedIDs: ["mcp-1"]),
            session: URLSession(configuration: Self.sessionConfiguration()))
        try await executor.configure([gated])
        ExternalMCPURLProtocol.handler = { request in
            Issue.record("Missing business instructions must not reach HTTP")
            return Self.response(request, body: #"{"result":{}}"#)
        }
        let result = try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation())
        #expect(Self.object(Self.object(result)?["content"])?["error"] == .string("external_mcp_skill_instructions_unavailable"))
    }

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
    func rejectsOversizedResponsesAndPreservesKnownJSONRPCErrorDetails() async throws {
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
        let failed = try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation())
        #expect(Self.object(failed)?["is_error"] == .bool(true))
        let content = try #require(Self.object(Self.object(failed)?["content"]))
        #expect(content["error"] == .string("external_mcp_rpc_error"))
        #expect(Self.object(content["detail"])?["message"] == .string("failed"))
        #expect(Self.object(content["detail"])?["code"] == .number(-1))
    }

    @Test
    func transportInterruptionStillThrowsRatherThanClaimingKnownFailure() async throws {
        let executor = NativeLocalAgentExternalMCPExecutor(host: ExternalMCPHostStub(selectedIDs: ["mcp-1"]),
            session: URLSession(configuration: Self.sessionConfiguration()))
        try await executor.configure([Self.config()])
        ExternalMCPURLProtocol.handler = { _ in throw URLError(.networkConnectionLost) }
        await #expect(throws: Error.self) {
            try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation())
        }
    }

    @Test
    func invalidArgumentsAreKnownFailuresWithoutHTTP() async throws {
        let executor = NativeLocalAgentExternalMCPExecutor(host: ExternalMCPHostStub(selectedIDs: ["mcp-1"]),
            session: URLSession(configuration: Self.sessionConfiguration()))
        try await executor.configure([Self.config()])
        ExternalMCPURLProtocol.handler = { request in
            Issue.record("Invalid arguments must not reach HTTP")
            return Self.response(request, body: #"{"result":{}}"#)
        }
        for arguments: LocalAgentJSONValue in [.object([:]), .object(["query": .bool(false)]), .array([])] {
            let result = try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation(arguments: arguments))
            #expect(Self.object(result)?["is_error"] == .bool(true))
            let content = try #require(Self.object(Self.object(result)?["content"]))
            #expect(content["error"] == .string("external_mcp_invalid_arguments"))
            guard case let .string(message)? = content["message"] else { Issue.record("Missing diagnostic"); continue }
            #expect(message.contains("$"))
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
                    "properties": .object(["query": .object(["type": .string("string")])]),
                    "required": .array([.string("query")]),
                    "additionalProperties": .bool(false),
                ])
            )]
        )
    }

    private static func invocation(arguments: LocalAgentJSONValue = .object(["query": .string("test")])) -> LocalAgentToolInvocationRecord {
        .init(
            invocationID: "invocation-1",
            runID: "run-1",
            batchID: "batch-1",
            callID: "call-1",
            toolName: "external_mcp__catalog__search",
            executionRoute: "platform",
            arguments: arguments,
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
    private let bundle: LocalAgentSkillBundleSnapshot?
    private let activatedRefs: [String]

    init(selectedIDs: [String], bundle: LocalAgentSkillBundleSnapshot? = nil, activatedRefs: [String] = []) {
        self.selectedIDs = selectedIDs
        self.bundle = bundle
        self.activatedRefs = activatedRefs
    }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: command)
        guard case let .object(object) = value else {
            throw CocoaError(.featureUnsupported)
        }
        if object["type"] == .string("get_run_skill_bundle") {
            let bundleValue = try bundle.map { try JSONDecoder().decode(LocalAgentJSONValue.self, from: JSONEncoder().encode($0)) } ?? .null
            return try JSONEncoder().encode(LocalAgentJSONValue.object([
                "type": .string("run_skill_bundle"), "run_id": .string("run-1"), "bundle": bundleValue,
            ]))
        }
        if object["type"] == .string("get_run_skill_activations") {
            return try JSONEncoder().encode(LocalAgentJSONValue.object([
                "type": .string("run_skill_activations"), "run_id": .string("run-1"),
                "skill_refs": .array(activatedRefs.map(LocalAgentJSONValue.string)),
            ]))
        }
        guard object["type"] == .string("get_run") else { throw CocoaError(.featureUnsupported) }
        return try JSONSerialization.data(withJSONObject: [
            "type": "run",
            "run": [
                "run_id": "run-1",
                "owner_user_id": "user-1",
                "owner_entity_type": "task",
                "owner_entity_id": "task-1",
                "profile_key": "task_execution",
                "capability_policy_revision": "policy-1",
                "input": [
                    "tool_options": [
                        "requires_execution": false,
                        "enabled_builtin_kinds": [],
                        "plugin_hints": [],
                        "plugin_release_snapshots": [],
                        "external_mcp_config_ids": selectedIDs,
                    ],
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
