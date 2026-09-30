import ChatOSCore
import Foundation

public actor NativeLocalAgentRemoteConnectionMetadataService: RemoteConnectionServicing {
    private let client: NativeLocalAgentRemoteConnectionClient?
    private var ownerUserID: String?
    private var versions: [String: UInt64] = [:]

    public init(host: (any LocalAgentHostClientServicing)?) {
        client = host.map(NativeLocalAgentRemoteConnectionClient.init(host:))
    }

    public func configure(ownerUserID: String) {
        self.ownerUserID = ownerUserID
        versions.removeAll()
    }

    public func reset() {
        ownerUserID = nil
        versions.removeAll()
    }

    public func listConnections() async throws -> [RemoteConnection] {
        let context = try requireContext()
        let records = try await context.client.list(ownerUserID: context.ownerUserID)
        records.forEach { versions[$0.connectionID] = $0.version }
        return records.map(Self.domain)
    }

    public func getConnection(id: String) async throws -> RemoteConnection? {
        let context = try requireContext()
        guard let record = try await context.client.get(
            ownerUserID: context.ownerUserID,
            connectionID: id
        ) else { return nil }
        versions[id] = record.version
        return Self.domain(record)
    }

    public func createConnection(_ draft: RemoteConnectionDraft) async throws -> RemoteConnection {
        let context = try requireContext()
        let record = try await context.client.create(ownerUserID: context.ownerUserID, draft: draft)
        versions[record.connectionID] = record.version
        return Self.domain(record)
    }

    public func updateConnection(
        id: String,
        draft: RemoteConnectionDraft
    ) async throws -> RemoteConnection {
        let context = try requireContext()
        let version = try await version(id: id, context: context)
        let record = try await context.client.update(
            ownerUserID: context.ownerUserID,
            connectionID: id,
            expectedVersion: version,
            draft: draft
        )
        versions[id] = record.version
        return Self.domain(record)
    }

    public func deleteConnection(id: String) async throws {
        let context = try requireContext()
        try await context.client.delete(
            ownerUserID: context.ownerUserID,
            connectionID: id,
            expectedVersion: try await version(id: id, context: context)
        )
        versions[id] = nil
    }

    public func testDraft(
        _: RemoteConnectionDraft,
        verificationCode _: String?
    ) async throws -> RemoteConnectionTestResult {
        throw NativeLocalAgentRemoteConnectionMetadataError.unsupportedTest
    }

    public func testSaved(
        id _: String,
        verificationCode _: String?
    ) async throws -> RemoteConnectionTestResult {
        throw NativeLocalAgentRemoteConnectionMetadataError.unsupportedTest
    }

    private func version(id: String, context: Context) async throws -> UInt64 {
        if let version = versions[id] { return version }
        guard let record = try await context.client.get(
            ownerUserID: context.ownerUserID,
            connectionID: id
        ) else { throw NativeLocalAgentRemoteConnectionMetadataError.notFound }
        versions[id] = record.version
        return record.version
    }

    private func requireContext() throws -> Context {
        guard let client, let ownerUserID else {
            throw NativeLocalAgentRemoteConnectionMetadataError.notConfigured
        }
        return Context(client: client, ownerUserID: ownerUserID)
    }

    private static func domain(_ record: LocalRemoteConnectionRecord) -> RemoteConnection {
        RemoteConnection(
            id: record.connectionID,
            name: record.name,
            host: record.host,
            port: Int(record.port),
            username: record.username,
            authenticationType: RemoteAuthenticationType(rawValue: record.authenticationType) ?? .privateKey,
            hasPassword: record.hasPassword,
            hasPrivateKeyPath: record.hasPrivateKeyPath,
            hasCertificatePath: record.hasCertificatePath,
            defaultRemotePath: record.defaultRemotePath,
            hostKeyPolicy: RemoteHostKeyPolicy(rawValue: record.hostKeyPolicy) ?? .strict,
            localConnectorDeviceID: record.localConnectorDeviceID,
            localConnectorWorkspaceID: record.localConnectorWorkspaceID,
            jumpEnabled: record.jumpEnabled,
            jumpConnectionID: record.jumpConnectionID,
            jumpHost: record.jumpHost,
            jumpPort: record.jumpPort.map(Int.init),
            jumpUsername: record.jumpUsername,
            hasJumpPrivateKeyPath: record.hasJumpPrivateKeyPath,
            hasJumpCertificatePath: record.hasJumpCertificatePath,
            hasJumpPassword: record.hasJumpPassword,
            lastActiveAt: record.lastActiveAtUnixMs.map {
                Date(timeIntervalSince1970: Double($0) / 1_000)
            }
        )
    }

    private struct Context: Sendable {
        let client: NativeLocalAgentRemoteConnectionClient
        let ownerUserID: String
    }
}

public enum NativeLocalAgentRemoteConnectionMetadataError: LocalizedError, Equatable {
    case notConfigured
    case notFound
    case unsupportedTest

    public var errorDescription: String? {
        switch self {
        case .notConfigured: "本地远端连接尚未连接到当前账号。"
        case .notFound: "远端连接不存在。"
        case .unsupportedTest: "元数据服务不执行 SSH 测试。"
        }
    }
}
