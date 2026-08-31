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
        allowedExitCodes: Set<Int32> = [0]
    ) throws -> NativeGitProcessOutput {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        process.arguments = arguments
        process.currentDirectoryURL = directory
        var environment = ProcessInfo.processInfo.environment
        environment["GIT_TERMINAL_PROMPT"] = "0"
        environment["LC_ALL"] = "C"
        process.environment = environment

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        let stdoutBuffer = NativeGitDataBuffer()
        let stderrBuffer = NativeGitDataBuffer()
        stdoutPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            if !data.isEmpty { stdoutBuffer.append(data) }
        }
        stderrPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            if !data.isEmpty { stderrBuffer.append(data) }
        }
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        try process.run()
        process.waitUntilExit()
        stdoutPipe.fileHandleForReading.readabilityHandler = nil
        stderrPipe.fileHandleForReading.readabilityHandler = nil
        stdoutBuffer.append(stdoutPipe.fileHandleForReading.readDataToEndOfFile())
        stderrBuffer.append(stderrPipe.fileHandleForReading.readDataToEndOfFile())
        let output = NativeGitProcessOutput(
            stdout: stdoutBuffer.data,
            stderr: stderrBuffer.data,
            exitCode: process.terminationStatus
        )
        guard allowedExitCodes.contains(output.exitCode) else {
            throw NativeGitError.commandFailed(
                arguments: arguments,
                message: output.stderrString.trimmingCharacters(in: .whitespacesAndNewlines)
            )
        }
        return output
    }
}

private final class NativeGitDataBuffer: @unchecked Sendable {
    private let lock = NSLock()
    private var value = Data()

    func append(_ data: Data) {
        lock.lock()
        value.append(data)
        lock.unlock()
    }

    var data: Data {
        lock.lock()
        defer { lock.unlock() }
        return value
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
