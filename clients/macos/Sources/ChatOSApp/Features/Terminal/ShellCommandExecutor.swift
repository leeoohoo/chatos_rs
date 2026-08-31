import Foundation

struct ShellCommandExecutor: TerminalCommandExecuting {
    func execute(command: String, workingDirectory: String) async -> TerminalCommandResult {
        await Task.detached(priority: .userInitiated) {
            let process = Process()
            let stdout = Pipe()
            let stderr = Pipe()
            process.executableURL = URL(fileURLWithPath: "/bin/zsh")
            process.arguments = ["-lc", command]
            process.currentDirectoryURL = URL(fileURLWithPath: workingDirectory)
            process.standardOutput = stdout
            process.standardError = stderr

            do {
                try process.run()
                process.waitUntilExit()
                let outputData = stdout.fileHandleForReading.readDataToEndOfFile()
                let errorData = stderr.fileHandleForReading.readDataToEndOfFile()
                return TerminalCommandResult(
                    output: String(decoding: outputData, as: UTF8.self).trimmingCharacters(in: .newlines),
                    error: String(decoding: errorData, as: UTF8.self).trimmingCharacters(in: .newlines),
                    exitCode: process.terminationStatus
                )
            } catch {
                return TerminalCommandResult(
                    output: "",
                    error: error.localizedDescription,
                    exitCode: -1
                )
            }
        }.value
    }
}
