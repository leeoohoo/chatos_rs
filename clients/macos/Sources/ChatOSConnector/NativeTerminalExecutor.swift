import ChatOSCore
import ChatOSProcessRuntime
import Darwin
import Foundation

enum NativeTerminalExecutor {
    static func execute(
        command: String,
        args: [String],
        cwd: String,
        workspace: LocalConnectorWorkspace,
        timeout: TimeInterval = 120
    ) async throws -> LocalConnectorTerminalResult {
        let resolvedRoot = URL(fileURLWithPath: workspace.absoluteRoot)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        let requestedURL = URL(fileURLWithPath: cwd, relativeTo: resolvedRoot)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        guard requestedURL.path == resolvedRoot.path
                || requestedURL.path.hasPrefix(resolvedRoot.pathWithTrailingSlash) else {
            throw NativeConnectorError.unsafeWorkingDirectory
        }

        let executable: String
        let processArguments: [String]
        if command.hasPrefix("/") {
            executable = command
            processArguments = [command] + args
        } else {
            executable = "/usr/bin/env"
            processArguments = [executable, command] + args
        }

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        let stdout = LockedDataBuffer()
        let stderr = LockedDataBuffer()
        NativeProcessPipeReader.install(
            on: stdoutPipe.fileHandleForReading,
            onData: stdout.append
        )
        NativeProcessPipeReader.install(
            on: stderrPipe.fileHandleForReading,
            onData: stderr.append
        )
        let nullInput = open("/dev/null", O_RDONLY)
        guard nullInput >= 0 else {
            stdoutPipe.fileHandleForReading.readabilityHandler = nil
            stderrPipe.fileHandleForReading.readabilityHandler = nil
            return .init(
                command: command,
                args: args,
                cwd: requestedURL.path,
                success: false,
                exitCode: nil,
                timedOut: false,
                stdout: stdout.string,
                stderr: stderr.string,
                error: "无法打开标准输入"
            )
        }
        defer { close(nullInput) }
        var processID: pid_t = 0
        let environment = ProcessInfo.processInfo.environment
        let spawnResult = withCStringArray(processArguments) { argv in
            withCStringArray(environment.map { "\($0.key)=\($0.value)" }) { envp in
                chatos_spawn_process_group(
                    executable,
                    argv,
                    envp,
                    requestedURL.path,
                    nullInput,
                    stdoutPipe.fileHandleForWriting.fileDescriptor,
                    stderrPipe.fileHandleForWriting.fileDescriptor,
                    &processID
                )
            }
        }
        stdoutPipe.fileHandleForWriting.closeFile()
        stderrPipe.fileHandleForWriting.closeFile()
        guard spawnResult == 0, processID > 0 else {
            stdoutPipe.fileHandleForReading.readabilityHandler = nil
            stderrPipe.fileHandleForReading.readabilityHandler = nil
            return .init(
                command: command,
                args: args,
                cwd: requestedURL.path,
                success: false,
                exitCode: nil,
                timedOut: false,
                stdout: stdout.string,
                stderr: stderr.string,
                error: "无法启动命令：\(String(cString: strerror(spawnResult)))"
            )
        }

        let exitSignal = NativeProcessExitSignal.reap(processID: processID)
        var exitCode = await withTaskCancellationHandler {
            await exitSignal.waitAsync(timeout: max(0, timeout))
        } onCancel: {
            _ = chatos_signal_process_group(processID, SIGKILL)
        }
        let timedOut = exitCode == nil
        if timedOut {
            _ = chatos_signal_process_group(processID, SIGTERM)
            exitCode = await exitSignal.waitAsync(timeout: 0.75)
            if exitCode == nil {
                _ = chatos_signal_process_group(processID, SIGKILL)
                exitCode = await exitSignal.waitAsync(timeout: 2)
            }
        }
        // The group leader may exit while a shell child remains alive.
        _ = chatos_signal_process_group(processID, SIGKILL)

        stdoutPipe.fileHandleForReading.readabilityHandler = nil
        stderrPipe.fileHandleForReading.readabilityHandler = nil
        stdout.append(stdoutPipe.fileHandleForReading.readDataToEndOfFile())
        stderr.append(stderrPipe.fileHandleForReading.readDataToEndOfFile())
        try Task.checkCancellation()
        return .init(
            command: command,
            args: args,
            cwd: requestedURL.path,
            success: !timedOut && exitCode == 0,
            exitCode: exitCode.map(Int.init),
            timedOut: timedOut,
            stdout: stdout.string,
            stderr: stderr.string,
            error: timedOut ? "命令执行超时，相关子进程已终止。" : nil
        )
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

private final class LockedDataBuffer: @unchecked Sendable {
    private static let maximumBytes = 512 * 1_024
    private let lock = NSLock()
    private var value = Data()

    func append(_ data: Data) {
        lock.lock()
        let remaining = Self.maximumBytes - value.count
        if remaining > 0 {
            value.append(data.prefix(remaining))
        }
        lock.unlock()
    }

    var string: String {
        lock.lock()
        let snapshot = value
        lock.unlock()
        return String(decoding: snapshot, as: UTF8.self)
    }
}

private extension URL {
    var pathWithTrailingSlash: String {
        path.hasSuffix("/") ? path : path + "/"
    }
}
