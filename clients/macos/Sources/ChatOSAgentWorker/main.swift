import ChatOSAgentRuntime
import ChatOSAgentWorkerProtocol
import Darwin
import Dispatch
import Foundation

private enum WorkerExit: Int32 {
    case usage = 64
    case protocolFailure = 65
    case parentExited = 70
    case ioFailure = 74
}

private struct WorkerTransportError: LocalizedError {
    let detail: String
    var errorDescription: String? { detail }
}

private func readLine(descriptor: Int32) throws -> Data {
    var result = Data()
    var byte: UInt8 = 0
    while result.count <= ChatOSAgentWorkerProtocol.maximumInlineBytes {
        let count = Darwin.read(descriptor, &byte, 1)
        if count == 1 {
            if byte == 0x0A { return result }
            result.append(byte)
        } else if count < 0, errno == EINTR {
            continue
        } else if count == 0 {
            throw WorkerTransportError(detail: "Agent worker input closed unexpectedly.")
        } else {
            throw WorkerTransportError(detail: "Agent worker input could not be read.")
        }
    }
    throw WorkerTransportError(detail: "Agent worker message exceeded the inline limit.")
}

private func writeAll(_ data: Data, descriptor: Int32) throws {
    try data.withUnsafeBytes { rawBuffer in
        guard let baseAddress = rawBuffer.baseAddress else { return }
        var offset = 0
        while offset < rawBuffer.count {
            let count = Darwin.write(
                descriptor,
                baseAddress.advanced(by: offset),
                rawBuffer.count - offset
            )
            if count > 0 {
                offset += count
            } else if count < 0, errno == EINTR {
                continue
            } else {
                throw WorkerTransportError(detail: "Agent worker output could not be written.")
            }
        }
    }
}

private actor AgentWorkerPeer {
    func send(_ message: AgentWorkerMessage) async throws {
        let data = try AgentWorkerWireCodec.encodeLine(message)
        try await Task.detached(priority: .userInitiated) {
            try writeAll(data, descriptor: STDOUT_FILENO)
        }.value
    }

    func receive() async throws -> AgentWorkerMessage {
        let data = try await Task.detached(priority: .userInitiated) {
            try readLine(descriptor: STDIN_FILENO)
        }.value
        return try AgentWorkerWireCodec.decode(data)
    }
}

private struct WorkerProxyModel: AgentModelClient {
    let peer: AgentWorkerPeer
    let serverSideCompaction: Bool

    var usesServerSideCompaction: Bool { serverSideCompaction }

    func complete(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval
    ) async throws -> AgentMessage {
        try await stream(messages: messages, tools: tools, timeout: timeout) { _ in }
    }

    func stream(
        messages: [AgentMessage],
        tools: [AgentToolDefinition],
        timeout: TimeInterval,
        onEvent: @escaping @Sendable (AgentModelStreamEvent) async -> Void
    ) async throws -> AgentMessage {
        let request = AgentWorkerModelRequest(
            id: UUID(),
            messages: messages,
            tools: tools,
            timeout: timeout
        )
        try await peer.send(.modelRequest(request))
        while true {
            switch try await peer.receive() {
            case let .modelEvent(event) where event.requestID == request.id:
                await onEvent(event.event)
            case let .modelResponse(response) where response.requestID == request.id:
                if let failure = response.failure {
                    throw AgentRemoteCallbackError(
                        detail: failure.detail,
                        transient: failure.transient
                    )
                }
                guard let message = response.message else {
                    throw AgentRuntimeError.invalidResponse
                }
                return message
            default:
                throw AgentRuntimeError.invalidResponse
            }
        }
    }
}

private struct WorkerProxyContextProvider: AgentContextProviding {
    let scope: AgentMemoryScope
    let peer: AgentWorkerPeer

    func prepare(
        checkpoint initial: AgentRunCheckpoint,
        tools: [AgentToolDefinition],
        policy: AgentContextPolicy,
        deadline: Date,
        synchronizeOnly: Bool,
        shouldPause: @escaping @Sendable () async -> Bool,
        record: @escaping AgentRuntime.Recorder
    ) async throws -> AgentPreparedContext {
        if await shouldPause() { throw CancellationError() }
        let request = AgentWorkerContextRequest(
            id: UUID(),
            checkpoint: initial,
            tools: tools,
            policy: policy,
            deadline: deadline,
            synchronizeOnly: synchronizeOnly
        )
        try await peer.send(.contextRequest(request))
        guard case let .contextResponse(response) = try await peer.receive(),
              response.requestID == request.id else {
            throw AgentRuntimeError.invalidResponse
        }
        if let contextFailure = response.contextFailure {
            throw AgentContextPreparationFailure(
                checkpoint: contextFailure.checkpoint,
                reason: contextFailure.reason,
                cancelled: contextFailure.cancelled
            )
        }
        if let failure = response.failure {
            throw AgentRemoteCallbackError(
                detail: failure.detail,
                transient: failure.transient
            )
        }
        guard let prepared = response.prepared else {
            throw AgentRuntimeError.invalidResponse
        }
        return prepared
    }
}

private func terminate(_ value: WorkerExit) -> Never {
    Darwin._exit(value.rawValue)
}

guard CommandLine.arguments.count == 3,
      CommandLine.arguments[1] == "--parent-pid",
      let parentProcessID = Int32(CommandLine.arguments[2]),
      parentProcessID > 1,
      getppid() == parentProcessID else {
    terminate(.usage)
}

_ = signal(SIGPIPE, SIG_IGN)
let parentMonitor = DispatchSource.makeProcessSource(
    identifier: parentProcessID,
    eventMask: .exit,
    queue: DispatchQueue.global(qos: .userInitiated)
)
parentMonitor.setEventHandler { terminate(.parentExited) }
parentMonitor.resume()
guard kill(parentProcessID, 0) == 0 || errno == EPERM else {
    terminate(.parentExited)
}

private let peer = AgentWorkerPeer()
do {
    guard case let .hello(hello) = try await peer.receive(),
          hello.parentProcessID == parentProcessID,
          hello.protocolMinVersion <= ChatOSAgentWorkerProtocol.maximumVersion,
          hello.protocolMaxVersion >= ChatOSAgentWorkerProtocol.minimumVersion else {
        terminate(.protocolFailure)
    }
    let workerInstanceID = UUID()
    try await peer.send(.helloResponse(.init(
        selectedProtocolVersion: ChatOSAgentWorkerProtocol.maximumVersion,
        workerInstanceID: workerInstanceID,
        runID: hello.runID,
        nonce: hello.nonce
    )))

    guard case let .start(start) = try await peer.receive(),
          start.checkpoint.id == hello.runID,
          start.checkpoint.scope == start.scope else {
        terminate(.protocolFailure)
    }
    let model = WorkerProxyModel(
        peer: peer,
        serverSideCompaction: start.modelUsesServerSideCompaction
    )
    let contextProvider: (any AgentContextProviding)? = if start.hasContextProvider {
        start.checkpoint.memory.map { WorkerProxyContextProvider(scope: $0.scope, peer: peer) }
    } else {
        nil
    }
    guard start.hasContextProvider == (contextProvider != nil) else {
        terminate(.protocolFailure)
    }

    let final = try await AgentRuntime().run(
        checkpoint: start.checkpoint,
        scope: start.scope,
        policy: start.policy,
        model: model,
        tools: start.tools,
        execute: { call in
            let request = AgentWorkerToolRequest(id: UUID(), call: call)
            try await peer.send(.toolRequest(request))
            guard case let .toolResponse(response) = try await peer.receive(),
                  response.requestID == request.id else {
                throw AgentRuntimeError.invalidResponse
            }
            if let failure = response.failure {
                throw AgentRemoteCallbackError(
                    detail: failure.detail,
                    transient: failure.transient
                )
            }
            guard let outcome = response.outcome else { throw AgentRuntimeError.invalidResponse }
            return outcome
        },
        completionCheck: {
            let request = AgentWorkerCompletionRequest(id: UUID())
            try await peer.send(.completionRequest(request))
            guard case let .completionResponse(response) = try await peer.receive(),
                  response.requestID == request.id else {
                throw AgentRuntimeError.invalidResponse
            }
            if let failure = response.failure {
                throw AgentRemoteCallbackError(
                    detail: failure.detail,
                    transient: failure.transient
                )
            }
            return response.result
        },
        contextProvider: contextProvider,
        record: { checkpoint, event in
            let request = AgentWorkerRecordRequest(
                id: UUID(),
                checkpoint: checkpoint,
                event: event
            )
            try await peer.send(.recordRequest(request))
            guard case let .recordResponse(response) = try await peer.receive(),
                  response.requestID == request.id else {
                throw AgentRuntimeError.invalidResponse
            }
            if let failure = response.failure {
                throw AgentRemoteCallbackError(
                    detail: failure.detail,
                    transient: failure.transient
                )
            }
        }
    )
    try await peer.send(.finished(.init(checkpoint: final)))
    parentMonitor.cancel()
    Darwin._exit(EXIT_SUCCESS)
} catch is CancellationError {
    parentMonitor.cancel()
    terminate(.parentExited)
} catch {
    try? await peer.send(.failed(.init(detail: error.localizedDescription)))
    parentMonitor.cancel()
    terminate(.ioFailure)
}
