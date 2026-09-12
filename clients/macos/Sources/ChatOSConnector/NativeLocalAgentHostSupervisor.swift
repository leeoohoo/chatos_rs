// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import Foundation

public enum NativeLocalAgentHostState: Equatable, Sendable {
    case stopped
    case starting(accountID: String)
    case running(accountID: String, processID: UInt32, clientEndpoint: String, restartCount: Int)
    case restarting(accountID: String, attempt: Int)
    case failed(accountID: String, reason: String)
}

public actor NativeLocalAgentHostSupervisor {
    public typealias ConfigurationProvider = @Sendable () async throws
        -> NativeLocalAgentHostLaunchConfiguration

    private let launcher: NativeLocalAgentHostProcessLauncher
    private let restartDelays: [Duration]
    private var desiredAccountID: String?
    private var configurationProvider: ConfigurationProvider?
    private var process: NativeLocalAgentHostProcess?
    private var monitor: Task<Void, Never>?
    private var generation: UInt64 = 0
    private var currentState: NativeLocalAgentHostState = .stopped

    public init(
        launcher: NativeLocalAgentHostProcessLauncher = .init(),
        restartDelays: [Duration] = [
            .seconds(1), .seconds(2), .seconds(5), .seconds(10), .seconds(30),
        ]
    ) throws {
        guard !restartDelays.isEmpty,
              restartDelays.allSatisfy({ $0 >= .zero && $0 <= .seconds(60) })
        else {
            throw NativeLocalAgentHostLaunchError.invalidConfiguration(
                "本地 Agent Host 重启策略无效"
            )
        }
        self.launcher = launcher
        self.restartDelays = restartDelays
    }

    public func state() -> NativeLocalAgentHostState { currentState }

    /// Starts one Host for the authenticated account. The provider is invoked
    /// again after a crash so every restart obtains fresh Keychain-backed
    /// credentials and a new launch frame instead of retaining or replaying
    /// secret bytes in this supervisor.
    public func start(
        accountID: String,
        configurationProvider: @escaping ConfigurationProvider
    ) async throws {
        guard !accountID.isEmpty,
              accountID == accountID.trimmingCharacters(in: .whitespacesAndNewlines)
        else {
            throw NativeLocalAgentHostLaunchError.invalidConfiguration(
                "本地 Agent Host 账户身份无效"
            )
        }
        await stopCurrentProcess()
        generation &+= 1
        desiredAccountID = accountID
        self.configurationProvider = configurationProvider
        currentState = .starting(accountID: accountID)
        do {
            try await launch(accountID: accountID, restartCount: 0, generation: generation)
        } catch {
            desiredAccountID = nil
            self.configurationProvider = nil
            currentState = .failed(accountID: accountID, reason: sanitizedReason(error))
            throw error
        }
    }

    /// Logging out revokes the desired account before terminating the Host,
    /// making its termination ineligible for restart.
    public func logout() async {
        desiredAccountID = nil
        configurationProvider = nil
        generation &+= 1
        monitor?.cancel()
        monitor = nil
        await stopCurrentProcess()
        currentState = .stopped
    }

    private func launch(
        accountID: String,
        restartCount: Int,
        generation expectedGeneration: UInt64
    ) async throws {
        guard desiredAccountID == accountID,
              generation == expectedGeneration,
              let configurationProvider
        else { return }
        let configuration = try await configurationProvider()
        guard desiredAccountID == accountID, generation == expectedGeneration else { return }
        let launched = try await launcher.launch(configuration)
        guard desiredAccountID == accountID, generation == expectedGeneration else {
            launched.terminate()
            return
        }
        process = launched
        currentState = .running(
            accountID: accountID,
            processID: launched.ready.processID,
            clientEndpoint: launched.ready.clientEndpoint,
            restartCount: restartCount
        )
        monitor?.cancel()
        monitor = Task { [weak self, launched] in
            let status = await launched.waitForExit()
            guard !Task.isCancelled else { return }
            await self?.processExited(
                launched,
                status: status,
                accountID: accountID,
                restartCount: restartCount,
                generation: expectedGeneration
            )
        }
    }

    private func processExited(
        _ exitedProcess: NativeLocalAgentHostProcess,
        status: Int32,
        accountID: String,
        restartCount: Int,
        generation expectedGeneration: UInt64
    ) async {
        guard process === exitedProcess,
              desiredAccountID == accountID,
              generation == expectedGeneration
        else { return }
        process = nil
        monitor = nil
        var lastReason = "Host 意外退出（状态码 \(status)）"
        for (offset, delay) in restartDelays.enumerated() {
            let attempt = restartCount + offset + 1
            currentState = .restarting(accountID: accountID, attempt: attempt)
            do {
                try await Task.sleep(for: delay)
                guard desiredAccountID == accountID, generation == expectedGeneration else { return }
                try await launch(
                    accountID: accountID,
                    restartCount: attempt,
                    generation: expectedGeneration
                )
                return
            } catch is CancellationError {
                return
            } catch {
                lastReason = sanitizedReason(error)
            }
        }
        guard desiredAccountID == accountID, generation == expectedGeneration else { return }
        desiredAccountID = nil
        configurationProvider = nil
        currentState = .failed(accountID: accountID, reason: lastReason)
    }

    private func stopCurrentProcess() async {
        monitor?.cancel()
        monitor = nil
        let previous = process
        process = nil
        previous?.terminate()
    }

    private func sanitizedReason(_ error: Error) -> String {
        if let error = error as? NativeLocalAgentHostLaunchError {
            return error.errorDescription ?? "本地 Agent Host 启动失败"
        }
        return "本地 Agent Host 启动失败"
    }
}
