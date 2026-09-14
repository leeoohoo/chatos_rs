// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Darwin
import Foundation

/// Resolves an already-running account Host from its deterministic private
/// Unix endpoint. An endpoint is attachable only when the socket metadata,
/// peer UID, peer PID/code signature, protocol version and owner all agree.
/// Unknown or mismatched live endpoints fail closed and are never replaced.
public struct NativeLocalAgentHostAttacher: Sendable {
    typealias IdentityVerifier = @Sendable (pid_t) throws -> Void

    private let identityVerifier: IdentityVerifier

    public init() {
        identityVerifier = NativeLocalAgentHostIdentity.validate(processID:)
    }

    init(testingIdentityVerifier: @escaping IdentityVerifier) {
        identityVerifier = testingIdentityVerifier
    }

    func attachIfRunning(
        accountID: String,
        configuration: NativeLocalAgentHostLaunchConfiguration
    ) async throws -> NativeLocalAgentHostProcess? {
        let endpoint = configuration.expectedClientEndpoint
        let transport = try NativeLocalAgentUnixTransport(
            socketPath: endpoint,
            ioTimeoutSeconds: 5,
            peerIdentityVerifier: identityVerifier
        )
        var verifiedProcessID: pid_t?
        do {
            let firstProcessID = try await transport.connectedPeerProcessID()
            verifiedProcessID = firstProcessID
            let client = try NativeLocalAgentIPCClient(
                ownerUserID: accountID,
                transport: transport
            )
            _ = try await client.uiEventCursor()
            let confirmedProcessID = try await transport.connectedPeerProcessID()
            guard firstProcessID == confirmedProcessID else {
                throw NativeLocalAgentIPCError.serverIdentityMismatch
            }
            return NativeLocalAgentHostProcess(
                attachedProcessID: confirmedProcessID,
                clientEndpoint: endpoint,
                identityVerifier: identityVerifier
            )
        } catch let error as NativeLocalAgentIPCError {
            switch error {
            case let .socketUnavailable(code) where code == ENOENT:
                return nil
            case let .socketUnavailable(code) where code == ECONNREFUSED:
                try removeStalePrivateSocket(at: endpoint)
                return nil
            case .protocolMismatch:
                // The endpoint is derived from this account and persistent
                // installation identity. The transport has also verified the
                // peer UID and that its code signature matches this App.
                // Therefore an older protocol here is a previous bundled Host,
                // not an unknown process. Replace it automatically during an
                // App upgrade instead of exposing an internal protocol error.
                guard let verifiedProcessID else { throw error }
                let obsoleteHost = NativeLocalAgentHostProcess(
                    attachedProcessID: verifiedProcessID,
                    clientEndpoint: endpoint,
                    identityVerifier: identityVerifier
                )
                _ = await obsoleteHost.stop()
                try removeStalePrivateSocket(at: endpoint)
                return nil
            default:
                throw error
            }
        }
    }

    private func removeStalePrivateSocket(at path: String) throws {
        let parent = URL(fileURLWithPath: path, isDirectory: false)
            .deletingLastPathComponent().path
        var parentMetadata = stat()
        guard lstat(parent, &parentMetadata) == 0,
              parentMetadata.st_mode & S_IFMT == S_IFDIR,
              parentMetadata.st_uid == geteuid(),
              parentMetadata.st_mode & 0o077 == 0
        else {
            throw NativeLocalAgentIPCError.serverIdentityMismatch
        }

        var socketMetadata = stat()
        guard lstat(path, &socketMetadata) == 0 else {
            if errno == ENOENT { return }
            throw NativeLocalAgentIPCError.socketUnavailable(errno)
        }
        guard socketMetadata.st_mode & S_IFMT == S_IFSOCK,
              socketMetadata.st_uid == geteuid(),
              socketMetadata.st_mode & 0o077 == 0
        else {
            throw NativeLocalAgentIPCError.serverIdentityMismatch
        }
        guard Darwin.unlink(path) == 0 || errno == ENOENT else {
            throw NativeLocalAgentIPCError.socketUnavailable(errno)
        }
    }
}
