import ChatOSCore
import Foundation

enum NativeTerminalExecutor {
    static func execute(
        command: String,
        args: [String],
        cwd: String,
        workspace: LocalConnectorWorkspace
    ) throws -> LocalConnectorTerminalResult {
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

        let process = Process()
        if command.hasPrefix("/") {
            process.executableURL = URL(fileURLWithPath: command)
            process.arguments = args
        } else {
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = [command] + args
        }
        process.currentDirectoryURL = requestedURL
        process.environment = ProcessInfo.processInfo.environment

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        let stdout = LockedDataBuffer()
        let stderr = LockedDataBuffer()
        stdoutPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            if !data.isEmpty { stdout.append(data) }
        }
        stderrPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            if !data.isEmpty { stderr.append(data) }
        }
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        do {
            try process.run()
            process.waitUntilExit()
        } catch {
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
                error: error.localizedDescription
            )
        }

        stdoutPipe.fileHandleForReading.readabilityHandler = nil
        stderrPipe.fileHandleForReading.readabilityHandler = nil
        stdout.append(stdoutPipe.fileHandleForReading.readDataToEndOfFile())
        stderr.append(stderrPipe.fileHandleForReading.readDataToEndOfFile())
        return .init(
            command: command,
            args: args,
            cwd: requestedURL.path,
            success: process.terminationStatus == 0,
            exitCode: Int(process.terminationStatus),
            timedOut: false,
            stdout: stdout.string,
            stderr: stderr.string,
            error: nil
        )
    }
}

private final class LockedDataBuffer: @unchecked Sendable {
    private let lock = NSLock()
    private var value = Data()

    func append(_ data: Data) {
        lock.lock()
        value.append(data)
        lock.unlock()
    }

    var string: String {
        lock.lock()
        let snapshot = value
        lock.unlock()
        return String(decoding: snapshot.prefix(512 * 1_024), as: UTF8.self)
    }
}

private extension URL {
    var pathWithTrailingSlash: String {
        path.hasSuffix("/") ? path : path + "/"
    }
}
