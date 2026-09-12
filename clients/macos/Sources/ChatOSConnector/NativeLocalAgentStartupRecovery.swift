// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public protocol NativeLocalAgentStartupRecoveryClient:
    NativeLocalAgentMainChatRestoreClient,
    NativeLocalAgentTaskStateClient
{}

extension NativeLocalAgentIPCClient: NativeLocalAgentStartupRecoveryClient {}

public enum NativeLocalAgentStartupRecoveryError: Error, Equatable, Sendable {
    case hostStopped
    case hostFailed(String)
    case endpointUnresponsive(String)
}

extension NativeLocalAgentStartupRecoveryError: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .hostStopped:
            "本地 Agent Host 在状态恢复完成前停止"
        case let .hostFailed(reason):
            "本地 Agent Host 恢复失败：\(reason)"
        case let .endpointUnresponsive(endpoint):
            "本地 Agent Host 已运行但 IPC 端点持续不可用：\(endpoint)"
        }
    }
}

/// Restores both native projections from one Host lifetime before event
/// consumption starts. Transient IPC loss is retried only when the Supervisor
/// still owns the desired Host; schema, identity and data errors fail closed.
public actor NativeLocalAgentStartupRecovery {
    public typealias ClientProvider = @Sendable () async throws
        -> any NativeLocalAgentStartupRecoveryClient
    public typealias StateProvider = @Sendable () async -> NativeLocalAgentHostState

    private let clientProvider: ClientProvider
    private let stateProvider: StateProvider
    private let mainChatRestorer: NativeLocalAgentMainChatRestorer
    private let taskEventSink: NativeLocalAgentTaskEventSink
    private let retryDelay: Duration
    private let maximumSameEndpointFailures: Int

    public init(
        clientProvider: @escaping ClientProvider,
        stateProvider: @escaping StateProvider,
        mainChatStore: any LocalAgentMainChatStateRestoring,
        taskStore: any LocalAgentTaskStateStoring,
        retryDelay: Duration = .milliseconds(250),
        maximumSameEndpointFailures: Int = 20
    ) {
        precondition(retryDelay >= .zero)
        precondition(maximumSameEndpointFailures > 0)
        self.clientProvider = clientProvider
        self.stateProvider = stateProvider
        self.retryDelay = retryDelay
        self.maximumSameEndpointFailures = maximumSameEndpointFailures
        self.mainChatRestorer = NativeLocalAgentMainChatRestorer(
            clientProvider: clientProvider,
            store: mainChatStore
        )
        self.taskEventSink = NativeLocalAgentTaskEventSink(
            clientProvider: clientProvider,
            store: taskStore
        )
    }

    public func restore() async throws -> NativeLocalAgentTaskEventSink {
        var failingEndpoint: String?
        var failuresAtEndpoint = 0

        while true {
            try Task.checkCancellation()
            let state = await stateProvider()
            switch state {
            case .starting, .restarting:
                failingEndpoint = nil
                failuresAtEndpoint = 0
                try await Task.sleep(for: retryDelay)
                continue
            case .stopped:
                throw NativeLocalAgentStartupRecoveryError.hostStopped
            case let .failed(_, reason):
                throw NativeLocalAgentStartupRecoveryError.hostFailed(reason)
            case let .running(_, _, endpoint, _):
                do {
                    let client = try await clientProvider()
                    // Both projections use exactly this client for the whole
                    // attempt, so a restart cannot splice two Host lifetimes.
                    async let mainChat: Void = mainChatRestorer.restore(using: client)
                    async let tasks: Void = taskEventSink.restore(using: client)
                    _ = try await (mainChat, tasks)
                    return taskEventSink
                } catch {
                    guard Self.isTransientHostLoss(error) else { throw error }
                    if failingEndpoint == endpoint {
                        failuresAtEndpoint += 1
                    } else {
                        failingEndpoint = endpoint
                        failuresAtEndpoint = 1
                    }
                    guard failuresAtEndpoint < maximumSameEndpointFailures else {
                        throw NativeLocalAgentStartupRecoveryError.endpointUnresponsive(endpoint)
                    }
                    try await Task.sleep(for: retryDelay)
                }
            }
        }
    }

    private static func isTransientHostLoss(_ error: Error) -> Bool {
        if let error = error as? NativeLocalAgentAccountSessionError {
            return error == .hostUnavailable
        }
        guard let error = error as? NativeLocalAgentIPCError else { return false }
        return switch error {
        case .socketUnavailable, .writeFailed, .connectionClosed:
            true
        case .invalidConfiguration, .serverIdentityMismatch, .responseFrameTooLarge,
             .invalidResponse, .protocolMismatch, .requestMismatch,
             .unexpectedResponse, .rejected:
            false
        }
    }
}
