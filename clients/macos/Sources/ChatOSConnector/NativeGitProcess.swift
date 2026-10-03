import ChatOSProcessRuntime
import Darwin
import Foundation

struct NativeGitProcessOutput: Sendable {
    var stdout: Data
    var stderr: Data
    var exitCode: Int32

    var stdoutString: String { String(decoding: stdout, as: UTF8.self) }
    var stderrString: String { String(decoding: stderr, as: UTF8.self) }
}

enum NativeGitProcess {
    static func run(
        arguments: [String],
        directory: URL,
        allowedExitCodes: Set<Int32> = [0],
        timeout: TimeInterval = 120,
        maximumOutputBytes: Int = 16 * 1_024 * 1_024
    ) throws -> NativeGitProcessOutput {
        var environment = ProcessInfo.processInfo.environment
        environment["GIT_TERMINAL_PROMPT"] = "0"
        environment["LC_ALL"] = "C"

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        let stdoutBuffer = NativeBoundedProcessOutput(maximumBytes: maximumOutputBytes)
        let stderrBuffer = NativeBoundedProcessOutput(maximumBytes: 1 * 1_024 * 1_024)
        NativeProcessPipeReader.install(
            on: stdoutPipe.fileHandleForReading,
            onData: stdoutBuffer.append
        )
        NativeProcessPipeReader.install(
            on: stderrPipe.fileHandleForReading,
            onData: stderrBuffer.append
        )
        let nullInput = open("/dev/null", O_RDONLY)
        guard nullInput >= 0 else {
            throw NativeGitError.commandFailed(
                arguments: arguments,
                message: "无法打开 Git 标准输入"
            )
        }
        defer { close(nullInput) }
        let executable = "/usr/bin/git"
        let processArguments = [executable] + arguments
        var processID: pid_t = 0
        let spawnResult = withCStringArray(processArguments) { argv in
            withCStringArray(environment.map { "\($0.key)=\($0.value)" }) { envp in
                chatos_spawn_process_group(
                    executable,
                    argv,
                    envp,
                    directory.path,
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
            throw NativeGitError.commandFailed(
                arguments: arguments,
                message: "无法启动 Git：\(String(cString: strerror(spawnResult)))"
            )
        }
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
        // Git may launch credential, transport, hook, or editor descendants.
        // A completed command must not leave any of them detached from ChatOS.
        _ = chatos_signal_process_group(processID, SIGKILL)
        guard let exitCode else {
            stdoutPipe.fileHandleForReading.readabilityHandler = nil
            stderrPipe.fileHandleForReading.readabilityHandler = nil
            try? stdoutPipe.fileHandleForReading.close()
            try? stderrPipe.fileHandleForReading.close()
            throw NativeGitError.commandTimedOut
        }
        stdoutPipe.fileHandleForReading.readabilityHandler = nil
        stderrPipe.fileHandleForReading.readabilityHandler = nil
        stdoutBuffer.append(stdoutPipe.fileHandleForReading.readDataToEndOfFile())
        stderrBuffer.append(stderrPipe.fileHandleForReading.readDataToEndOfFile())
        let stdout = stdoutBuffer.snapshot
        let stderr = stderrBuffer.snapshot
        if timedOut {
            throw NativeGitError.commandTimedOut
        }
        guard !stdout.discarded, !stderr.discarded else {
            throw NativeGitError.outputTooLarge
        }
        let output = NativeGitProcessOutput(
            stdout: stdout.data,
            stderr: stderr.data,
            exitCode: exitCode
        )
        guard allowedExitCodes.contains(output.exitCode) else {
            throw NativeGitError.commandFailed(
                arguments: arguments,
                message: output.stderrString.trimmingCharacters(in: .whitespacesAndNewlines)
            )
        }
        return output
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

enum NativeGitError: LocalizedError, Equatable {
    case notRepository
    case repositoryOutsideWorkspace
    case invalidBranchName
    case emptyCommitMessage
    case noRemote
    case noCurrentBranch
    case invalidRemote
    case commandTimedOut
    case outputTooLarge
    case commandFailed(arguments: [String], message: String)

    var errorDescription: String? {
        switch self {
        case .notRepository:
            "这个项目目录还不是 Git 仓库。"
        case .repositoryOutsideWorkspace:
            "Git 仓库根目录超出了当前项目允许访问的本机工作区。"
        case .invalidBranchName:
            "分支名称不符合 Git 规则，请换一个名称。"
        case .emptyCommitMessage:
            "请输入提交说明。"
        case .noRemote:
            "这个仓库还没有配置远程仓库。"
        case .noCurrentBranch:
            "当前处于分离 HEAD 状态，不能直接发布分支。"
        case .invalidRemote:
            "远程仓库名称和地址不能为空。"
        case .commandTimedOut:
            "Git 命令执行超时，相关子进程已终止。"
        case .outputTooLarge:
            "Git 命令输出过大，请缩小操作范围后重试。"
        case let .commandFailed(_, message):
            localizedCommandMessage(message)
        }
    }

    private func localizedCommandMessage(_ message: String) -> String {
        let detail = message.isEmpty ? "Git 命令执行失败。" : message
        if detail.contains("Your local changes to the following files would be overwritten") {
            return "当前修改会被分支切换覆盖，请先提交或暂存这些修改。"
        }
        if detail.contains("CONFLICT") || detail.contains("Automatic merge failed") {
            return "分支已进入冲突状态。请先处理冲突文件，再完成提交。"
        }
        if detail.contains("no upstream branch") {
            return "当前分支还没有关联远程分支，请先发布分支。"
        }
        if detail.contains("nothing to commit") {
            return "没有可提交的暂存修改。"
        }
        return detail
    }
}
