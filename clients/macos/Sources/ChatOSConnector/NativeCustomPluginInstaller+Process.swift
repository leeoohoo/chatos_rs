import ChatOSCore
import ChatOSProcessRuntime
import Darwin
import Dispatch
import Foundation

extension NativePluginInstaller {
    /// npm can remain alive for minutes while waiting on the network or package extraction. Keep
    /// those synchronous process waits on concurrent GCD lanes so they never consume Swift's
    /// cooperative executor or serialize unrelated plugin operations.
    private static let npmBlockingWorkQueue = DispatchQueue(
        label: "com.chatos.plugin-installer.npm-blocking-work",
        qos: .utility,
        attributes: .concurrent
    )

    static func installNPMDependencies(in packageRoot: URL) async throws {
        try await runInstallerProcess(
            executableURL: resolveExecutable(named: "npm"),
            arguments: [
                "install", "--ignore-scripts", "--omit=dev", "--no-audit", "--no-fund",
                "--save-exact", "--package-lock=true",
            ],
            currentDirectoryURL: packageRoot
        )
    }

    static func runInstallerProcess(
        executableURL: URL,
        arguments: [String],
        currentDirectoryURL: URL
    ) async throws {
        _ = try await runNPMProcess(
            executableURL: executableURL,
            arguments: arguments,
            currentDirectoryURL: currentDirectoryURL,
            timeout: 5 * 60,
            stderrLimit: 2 * 1_024 * 1_024,
            overflowMessage: "npm 输出超过安全限制",
            timeoutMessage: "npm 安装超过 5 分钟，已终止",
            failureLabel: "npm 安装"
        )
    }

    static func runNPMQuery(arguments: [String]) async throws -> Data {
        let allowedCommands = Set(["search", "view"])
        guard let command = arguments.first,
              allowedCommands.contains(command),
              arguments.count <= 16,
              arguments.allSatisfy({ !$0.contains("\0") && $0.count <= 2_048 }) else {
            throw NativeConnectorError.pluginInstallation("npm 查询参数无效")
        }
        return try await runNPMProcess(
            executableURL: resolveExecutable(named: "npm"),
            arguments: arguments,
            currentDirectoryURL: nil,
            timeout: 60,
            stderrLimit: 256 * 1_024,
            overflowMessage: "npm 查询输出超过安全限制",
            timeoutMessage: "npm 查询超过 60 秒，已终止",
            failureLabel: "npm 查询"
        )
    }

    /// Internal rather than private so process isolation can be verified with deterministic local
    /// fixtures without invoking the network or a user's npm configuration.
    static func runNPMProcess(
        executableURL: URL,
        arguments: [String],
        currentDirectoryURL: URL?,
        timeout: TimeInterval,
        stderrLimit: Int,
        overflowMessage: String,
        timeoutMessage: String,
        failureLabel: String
    ) async throws -> Data {
        let controller = NativeNPMProcessController()
        return try await withTaskCancellationHandler {
            do {
                let output: Data = try await withCheckedThrowingContinuation { continuation in
                    npmBlockingWorkQueue.async {
                        do {
                            try controller.throwIfCancelled()
                            continuation.resume(returning: try runNPMProcessBlocking(
                                executableURL: executableURL,
                                arguments: arguments,
                                currentDirectoryURL: currentDirectoryURL,
                                timeout: timeout,
                                stderrLimit: stderrLimit,
                                overflowMessage: overflowMessage,
                                timeoutMessage: timeoutMessage,
                                failureLabel: failureLabel,
                                controller: controller
                            ))
                        } catch {
                            continuation.resume(throwing: error)
                        }
                    }
                }
                try Task.checkCancellation()
                return output
            } catch {
                if Task.isCancelled { throw CancellationError() }
                throw error
            }
        } onCancel: {
            controller.cancel()
        }
    }

    private static func runNPMProcessBlocking(
        executableURL: URL,
        arguments: [String],
        currentDirectoryURL: URL?,
        timeout: TimeInterval,
        stderrLimit: Int,
        overflowMessage: String,
        timeoutMessage: String,
        failureLabel: String,
        controller: NativeNPMProcessController
    ) throws -> Data {
        try controller.throwIfCancelled()
        let stdout = Pipe()
        let stderr = Pipe()
        let stdoutCapture = NativeBoundedProcessOutput(maximumBytes: 2 * 1_024 * 1_024)
        let stderrCapture = NativeBoundedProcessOutput(maximumBytes: stderrLimit)
        NativeProcessPipeReader.install(
            on: stdout.fileHandleForReading,
            onData: stdoutCapture.append
        )
        NativeProcessPipeReader.install(
            on: stderr.fileHandleForReading,
            onData: stderrCapture.append
        )
        let nullInput = open("/dev/null", O_RDONLY)
        guard nullInput >= 0 else {
            stdout.fileHandleForReading.readabilityHandler = nil
            stderr.fileHandleForReading.readabilityHandler = nil
            throw NativeConnectorError.pluginInstallation("无法打开 npm 标准输入")
        }
        defer { close(nullInput) }

        let executable = executableURL.path
        let processArguments = [executable] + arguments
        let environment = ProcessInfo.processInfo.environment
        var processID: pid_t = 0
        let spawnResult = withCStringArray(processArguments) { argv in
            withCStringArray(environment.map { "\($0.key)=\($0.value)" }) { envp in
                chatos_spawn_process_group(
                    executable,
                    argv,
                    envp,
                    currentDirectoryURL?.path,
                    nullInput,
                    stdout.fileHandleForWriting.fileDescriptor,
                    stderr.fileHandleForWriting.fileDescriptor,
                    &processID
                )
            }
        }
        stdout.fileHandleForWriting.closeFile()
        stderr.fileHandleForWriting.closeFile()
        guard spawnResult == 0, processID > 0 else {
            stdout.fileHandleForReading.readabilityHandler = nil
            stderr.fileHandleForReading.readabilityHandler = nil
            throw NativeConnectorError.pluginInstallation(
                "无法启动 npm：\(String(cString: strerror(spawnResult)))"
            )
        }

        let registration = controller.register(processID)
        defer { controller.clear(processID, registration: registration) }
        let exitSignal = NativeProcessExitSignal.reap(processID: processID)
        var exitCode = exitSignal.wait(timeout: timeout)
        let timedOut = exitCode == nil
        if timedOut {
            _ = chatos_signal_process_group(processID, SIGTERM)
            exitCode = exitSignal.wait(timeout: 0.75)
            if exitCode == nil {
                _ = chatos_signal_process_group(processID, SIGKILL)
                exitCode = exitSignal.wait(timeout: 2)
            }
        }
        // npm can launch lifecycle descendants even with --ignore-scripts. Once the owned command
        // has ended, close its complete process group before draining the final pipe bytes.
        _ = chatos_signal_process_group(processID, SIGKILL)
        guard let exitCode else {
            stdout.fileHandleForReading.readabilityHandler = nil
            stderr.fileHandleForReading.readabilityHandler = nil
            try? stdout.fileHandleForReading.close()
            try? stderr.fileHandleForReading.close()
            throw NativeConnectorError.pluginInstallation("npm 进程无法在终止后退出")
        }

        stdout.fileHandleForReading.readabilityHandler = nil
        stderr.fileHandleForReading.readabilityHandler = nil
        stdoutCapture.append(stdout.fileHandleForReading.readDataToEndOfFile())
        stderrCapture.append(stderr.fileHandleForReading.readDataToEndOfFile())
        let output = stdoutCapture.snapshot
        let errorOutput = stderrCapture.snapshot
        guard !output.discarded, !errorOutput.discarded else {
            throw NativeConnectorError.pluginInstallation(overflowMessage)
        }
        if controller.isCancelled { throw CancellationError() }
        guard !timedOut else {
            throw NativeConnectorError.pluginInstallation(timeoutMessage)
        }
        guard exitCode == 0 else {
            let detail = String(decoding: errorOutput.data.suffix(8 * 1_024), as: UTF8.self)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            throw NativeConnectorError.pluginInstallation(
                detail.isEmpty ? "\(failureLabel)失败" : "\(failureLabel)失败：\(detail)"
            )
        }
        return output.data
    }

    private static func withCStringArray<Result>(
        _ values: [String],
        _ body: ([UnsafeMutablePointer<CChar>?]) throws -> Result
    ) rethrows -> Result {
        let pointers = values.map { strdup($0) }
        defer { pointers.forEach { free($0) } }
        return try body(pointers + [nil])
    }
}

final class NativeNPMProcessController: @unchecked Sendable {
    private static let forceKillQueue = DispatchQueue(
        label: "com.chatos.plugin-installer.npm-process-termination",
        qos: .utility,
        attributes: .concurrent
    )

    private let lock = NSLock()
    private var cancellationRequested = false
    private var activeProcessID: pid_t?
    private var activeRegistration: UUID?

    var isCancelled: Bool {
        lock.lock()
        defer { lock.unlock() }
        return cancellationRequested
    }

    func throwIfCancelled() throws {
        if isCancelled { throw CancellationError() }
    }

    func register(_ processID: pid_t) -> UUID {
        let registration = UUID()
        lock.lock()
        activeProcessID = processID
        activeRegistration = registration
        let cancelled = cancellationRequested
        lock.unlock()
        if cancelled {
            requestTermination(of: processID, registration: registration)
        }
        return registration
    }

    func clear(_ processID: pid_t, registration: UUID) {
        lock.lock()
        if activeProcessID == processID, activeRegistration == registration {
            activeProcessID = nil
            activeRegistration = nil
        }
        lock.unlock()
    }

    func cancel() {
        lock.lock()
        cancellationRequested = true
        let processID = activeProcessID
        let registration = activeRegistration
        lock.unlock()
        if let processID, let registration {
            requestTermination(of: processID, registration: registration)
        }
    }

    private func requestTermination(of processID: pid_t, registration: UUID) {
        _ = chatos_signal_process_group(processID, SIGTERM)
        Self.forceKillQueue.asyncAfter(deadline: .now() + 2) { [weak self] in
            guard let self, self.owns(processID, registration: registration) else { return }
            _ = chatos_signal_process_group(processID, SIGKILL)
        }
    }

    private func owns(_ processID: pid_t, registration: UUID) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return activeProcessID == processID && activeRegistration == registration
    }
}
