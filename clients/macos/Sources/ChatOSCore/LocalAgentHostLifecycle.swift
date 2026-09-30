import Foundation

public protocol LocalAgentHostLifecycleServicing: Sendable {
    func start(ownerUserID: String) async throws

    func stop() async
}

/// Serialized command access to the authenticated Local Agent Host process.
///
/// `command` is the JSON value stored inside the IPC envelope's `command`
/// field. The returned data is the successful `result` JSON value. Envelope
/// identity, protocol version and host errors are handled by the transport.
public protocol LocalAgentHostClientServicing: LocalAgentHostLifecycleServicing {
    func request(command: Data) async throws -> Data
}
