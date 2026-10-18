import ChatOSCore
import ChatOSProcessRuntime
import Darwin
import Foundation

extension NativePluginInstaller {
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

    private static func runNPMProcess(
        executableURL: URL,
        arguments: [String],
        currentDirectoryURL: URL?,
        timeout: TimeInterval,
        stderrLimit: Int,
        overflowMessage: String,
        timeoutMessage: String,
        failureLabel: String
    ) async throws -> Data {
        let worker = Task.detached(priority: .utility) {
            let process = Process()
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
            process.executableURL = executableURL
            process.arguments = arguments
            process.currentDirectoryURL = currentDirectoryURL
            process.standardOutput = stdout
            process.standardError = stderr
            let deadline = Date().addingTimeInterval(timeout)
            var timedOut = false
            do {
                try process.run()
                while process.isRunning, !Task.isCancelled, Date() < deadline {
                    try? await Task.sleep(for: .milliseconds(50))
                }
                if process.isRunning {
                    timedOut = !Task.isCancelled && Date() >= deadline
                    process.terminate()
                    let terminationDeadline = Date().addingTimeInterval(2)
                    while process.isRunning, Date() < terminationDeadline {
                        try? await Task.sleep(for: .milliseconds(50))
                    }
                    if process.isRunning { kill(process.processIdentifier, SIGKILL) }
                    process.waitUntilExit()
                }
            } catch {
                stdout.fileHandleForReading.readabilityHandler = nil
                stderr.fileHandleForReading.readabilityHandler = nil
                throw error
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
            if Task.isCancelled { throw CancellationError() }
            guard !timedOut else {
                throw NativeConnectorError.pluginInstallation(timeoutMessage)
            }
            guard process.terminationStatus == 0 else {
                let detail = String(decoding: errorOutput.data.suffix(8 * 1_024), as: UTF8.self)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                throw NativeConnectorError.pluginInstallation(
                    detail.isEmpty ? "\(failureLabel)失败" : "\(failureLabel)失败：\(detail)"
                )
            }
            return output.data
        }
        return try await withTaskCancellationHandler {
            try await worker.value
        } onCancel: {
            worker.cancel()
        }
    }
}
