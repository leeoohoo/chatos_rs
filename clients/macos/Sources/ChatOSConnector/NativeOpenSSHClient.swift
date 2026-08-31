import ChatOSCore
import Foundation

protocol NativeRemoteSSHExecuting: Sendable {
    func runCommand(
        draft: RemoteConnectionDraft,
        command: String,
        timeoutSeconds: Int,
        maximumOutputCharacters: Int
    ) async throws -> NativeRemoteCommandResult

    func listDirectory(
        draft: RemoteConnectionDraft,
        path: String,
        limit: Int
    ) async throws -> [NativeRemoteDirectoryEntry]

    func resolveDirectory(
        draft: RemoteConnectionDraft,
        path: String
    ) async throws -> String

    func download(
        draft: RemoteConnectionDraft,
        path: String,
        maximumBytes: Int
    ) async throws -> Data

    func upload(
        draft: RemoteConnectionDraft,
        path: String,
        data: Data,
        createParentDirectories: Bool,
        overwrite: Bool
    ) async throws

    func uploadFile(
        draft: RemoteConnectionDraft,
        localURL: URL,
        remotePath: String,
        overwrite: Bool
    ) async throws

    func downloadFile(
        draft: RemoteConnectionDraft,
        remotePath: String,
        localURL: URL,
        overwrite: Bool
    ) async throws

    func createDirectory(draft: RemoteConnectionDraft, path: String) async throws
    func renameEntry(
        draft: RemoteConnectionDraft,
        path: String,
        destinationPath: String
    ) async throws
    func deleteEntry(
        draft: RemoteConnectionDraft,
        path: String,
        recursively: Bool
    ) async throws
}

struct NativeRemoteCommandResult: Sendable {
    let exitCode: Int
    let stdout: String
    let stderr: String
    let truncated: Bool
    let timedOut: Bool
}

struct NativeRemoteDirectoryEntry: Sendable {
    let name: String
    let path: String
    let type: String
    let size: Int64?
    let modifiedAt: Date?
    let permissions: String?
}

struct NativeOpenSSHClient: NativeRemoteSSHExecuting {
    func runCommand(
        draft: RemoteConnectionDraft,
        command: String,
        timeoutSeconds: Int,
        maximumOutputCharacters: Int
    ) async throws -> NativeRemoteCommandResult {
        try await Task.detached(priority: .userInitiated) {
            try Self.withRuntime(draft: draft) { runtime in
                let captured = try Self.run(
                    executable: "/usr/bin/ssh",
                    arguments: ["-F", runtime.config.path, "chatos-target", command],
                    environment: runtime.environment,
                    input: nil,
                    timeout: TimeInterval(timeoutSeconds),
                    maximumCapturedBytes: max(maximumOutputCharacters * 4, 64 * 1_024)
                )
                let stdout = Self.truncate(String(decoding: captured.stdout, as: UTF8.self), maximum: maximumOutputCharacters)
                let stderr = Self.truncate(String(decoding: captured.stderr, as: UTF8.self), maximum: maximumOutputCharacters)
                return NativeRemoteCommandResult(
                    exitCode: captured.exitCode,
                    stdout: stdout.text,
                    stderr: stderr.text,
                    truncated: stdout.truncated || stderr.truncated || captured.outputDiscarded,
                    timedOut: captured.timedOut
                )
            }
        }.value
    }

    func listDirectory(
        draft: RemoteConnectionDraft,
        path: String,
        limit: Int
    ) async throws -> [NativeRemoteDirectoryEntry] {
        try await Task.detached(priority: .userInitiated) {
            try Self.withRuntime(draft: draft) { runtime in
                let normalizedLimit = min(max(limit, 1), 1_000)
                let quotedPath = Self.shellQuote(path)
                let script = """
                dir=\(quotedPath)
                [ -d "$dir" ] || exit 66
                count=0
                for item in "$dir"/* "$dir"/.[!.]* "$dir"/..?*; do
                  [ -e "$item" ] || [ -L "$item" ] || continue
                  if [ -L "$item" ]; then kind=symlink
                  elif [ -d "$item" ]; then kind=directory
                  elif [ -f "$item" ]; then kind=file
                  else kind=other
                  fi
                  name=${item##*/}
                  if stat -c '%s' -- "$item" >/dev/null 2>&1; then
                    size=$(stat -c '%s' -- "$item" 2>/dev/null || true)
                    modified=$(stat -c '%Y' -- "$item" 2>/dev/null || true)
                    permissions=$(stat -c '%A' -- "$item" 2>/dev/null || true)
                  else
                    size=$(stat -f '%z' "$item" 2>/dev/null || true)
                    modified=$(stat -f '%m' "$item" 2>/dev/null || true)
                    permissions=$(stat -f '%Sp' "$item" 2>/dev/null || true)
                  fi
                  printf '%s\\0%s\\0%s\\0%s\\0%s\\0' "$kind" "$name" "$size" "$modified" "$permissions"
                  count=$((count + 1))
                  [ "$count" -ge \(normalizedLimit) ] && break
                done
                """
                let captured = try Self.run(
                    executable: "/usr/bin/ssh",
                    arguments: ["-F", runtime.config.path, "chatos-target", script],
                    environment: runtime.environment,
                    input: nil,
                    timeout: 20,
                    maximumCapturedBytes: 2 * 1_024 * 1_024
                )
                guard captured.exitCode == 0, !captured.timedOut else {
                    throw NativeOpenSSHError.remoteFailure(Self.failureMessage(captured))
                }
                let fields = captured.stdout.split(separator: 0, omittingEmptySubsequences: false)
                var entries: [NativeRemoteDirectoryEntry] = []
                var index = 0
                while index + 4 < fields.count, entries.count < normalizedLimit {
                    let type = String(decoding: fields[index], as: UTF8.self)
                    let name = String(decoding: fields[index + 1], as: UTF8.self)
                    let size = Int64(String(decoding: fields[index + 2], as: UTF8.self))
                    let modifiedSeconds = TimeInterval(String(decoding: fields[index + 3], as: UTF8.self))
                    let permissions = String(decoding: fields[index + 4], as: UTF8.self)
                    if !name.isEmpty {
                        entries.append(.init(
                            name: name,
                            path: Self.joinRemotePath(path, name),
                            type: type,
                            size: size,
                            modifiedAt: modifiedSeconds.map(Date.init(timeIntervalSince1970:)),
                            permissions: permissions.isEmpty ? nil : permissions
                        ))
                    }
                    index += 5
                }
                return entries
            }
        }.value
    }

    func resolveDirectory(
        draft: RemoteConnectionDraft,
        path: String
    ) async throws -> String {
        try await Task.detached(priority: .userInitiated) {
            try Self.withRuntime(draft: draft) { runtime in
                let script = """
                candidate=\(Self.shellQuote(path))
                case "$candidate" in
                  '~') candidate="$HOME" ;;
                  '~/'*) candidate="$HOME/${candidate#~/}" ;;
                esac
                cd -- "$candidate" && pwd -P
                """
                let captured = try Self.run(
                    executable: "/usr/bin/ssh",
                    arguments: ["-F", runtime.config.path, "chatos-target", script],
                    environment: runtime.environment,
                    input: nil,
                    timeout: 20,
                    maximumCapturedBytes: 64 * 1_024
                )
                guard captured.exitCode == 0, !captured.timedOut else {
                    throw NativeOpenSSHError.remoteFailure(Self.failureMessage(captured))
                }
                let resolved = String(decoding: captured.stdout, as: UTF8.self)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                guard !resolved.isEmpty else {
                    throw NativeOpenSSHError.remoteFailure("无法确定远端目录。")
                }
                return resolved
            }
        }.value
    }

    func download(
        draft: RemoteConnectionDraft,
        path: String,
        maximumBytes: Int
    ) async throws -> Data {
        try await Task.detached(priority: .userInitiated) {
            try Self.withRuntime(draft: draft) { runtime in
                let limit = min(max(maximumBytes, 1), 256 * 1_024)
                let command = "test -f \(Self.shellQuote(path)) && head -c \(limit + 1) -- \(Self.shellQuote(path))"
                let captured = try Self.run(
                    executable: "/usr/bin/ssh",
                    arguments: ["-F", runtime.config.path, "chatos-target", command],
                    environment: runtime.environment,
                    input: nil,
                    timeout: 20,
                    maximumCapturedBytes: limit + 1
                )
                guard captured.exitCode == 0, !captured.timedOut else {
                    throw NativeOpenSSHError.remoteFailure(Self.failureMessage(captured))
                }
                guard captured.stdout.count <= limit, !captured.outputDiscarded else {
                    throw NativeOpenSSHError.fileTooLarge(limit)
                }
                return captured.stdout
            }
        }.value
    }

    func upload(
        draft: RemoteConnectionDraft,
        path: String,
        data: Data,
        createParentDirectories: Bool,
        overwrite: Bool
    ) async throws {
        try await Task.detached(priority: .userInitiated) {
            try Self.withRuntime(draft: draft) { runtime in
                let quotedPath = Self.shellQuote(path)
                let parent = Self.shellQuote(Self.remoteParent(path))
                var commands = ["set -e"]
                if createParentDirectories { commands.append("mkdir -p -- \(parent)") }
                if !overwrite { commands.append("[ ! -e \(quotedPath) ]") }
                commands.append("cat > \(quotedPath)")
                let captured = try Self.run(
                    executable: "/usr/bin/ssh",
                    arguments: ["-F", runtime.config.path, "chatos-target", commands.joined(separator: "; ")],
                    environment: runtime.environment,
                    input: data,
                    timeout: 30,
                    maximumCapturedBytes: 256 * 1_024
                )
                guard captured.exitCode == 0, !captured.timedOut else {
                    throw NativeOpenSSHError.remoteFailure(Self.failureMessage(captured))
                }
            }
        }.value
    }

    func uploadFile(
        draft: RemoteConnectionDraft,
        localURL: URL,
        remotePath: String,
        overwrite: Bool
    ) async throws {
        try await Task.detached(priority: .userInitiated) {
            guard FileManager.default.isReadableFile(atPath: localURL.path) else {
                throw NativeOpenSSHError.localFileUnavailable(localURL.path)
            }
            try Self.withRuntime(draft: draft) { runtime in
                if !overwrite {
                    try Self.requireRemotePathMissing(remotePath, runtime: runtime)
                }
                try Self.runRemoteMutation(
                    "mkdir -p -- \(Self.shellQuote(Self.remoteParent(remotePath)))",
                    runtime: runtime
                )
                try Self.runSFTPTransfer(
                    arguments: [localURL.path, "chatos-target:\(remotePath)"],
                    runtime: runtime
                )
            }
        }.value
    }

    func downloadFile(
        draft: RemoteConnectionDraft,
        remotePath: String,
        localURL: URL,
        overwrite: Bool
    ) async throws {
        try await Task.detached(priority: .userInitiated) {
            if FileManager.default.fileExists(atPath: localURL.path), !overwrite {
                throw NativeOpenSSHError.localFileExists(localURL.path)
            }
            try FileManager.default.createDirectory(
                at: localURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try Self.withRuntime(draft: draft) { runtime in
                try Self.runSFTPTransfer(
                    arguments: ["chatos-target:\(remotePath)", localURL.path],
                    runtime: runtime
                )
            }
        }.value
    }

    func createDirectory(draft: RemoteConnectionDraft, path: String) async throws {
        try await mutate(draft: draft, command: "mkdir -- \(Self.shellQuote(path))")
    }

    func renameEntry(
        draft: RemoteConnectionDraft,
        path: String,
        destinationPath: String
    ) async throws {
        try await mutate(
            draft: draft,
            command: "mv -- \(Self.shellQuote(path)) \(Self.shellQuote(destinationPath))"
        )
    }

    func deleteEntry(
        draft: RemoteConnectionDraft,
        path: String,
        recursively: Bool
    ) async throws {
        let command = recursively
            ? "rm -rf -- \(Self.shellQuote(path))"
            : "rm -- \(Self.shellQuote(path))"
        try await mutate(draft: draft, command: command)
    }

    private func mutate(draft: RemoteConnectionDraft, command: String) async throws {
        try await Task.detached(priority: .userInitiated) {
            try Self.withRuntime(draft: draft) { runtime in
                try Self.runRemoteMutation(command, runtime: runtime)
            }
        }.value
    }

    private static func runRemoteMutation(_ command: String, runtime: SSHRuntime) throws {
        let captured = try run(
            executable: "/usr/bin/ssh",
            arguments: ["-F", runtime.config.path, "chatos-target", "set -e; \(command)"],
            environment: runtime.environment,
            input: nil,
            timeout: 30,
            maximumCapturedBytes: 256 * 1_024
        )
        guard captured.exitCode == 0, !captured.timedOut else {
            throw NativeOpenSSHError.remoteFailure(failureMessage(captured))
        }
    }

    private static func requireRemotePathMissing(_ path: String, runtime: SSHRuntime) throws {
        let quotedPath = shellQuote(path)
        let captured = try run(
            executable: "/usr/bin/ssh",
            arguments: [
                "-F", runtime.config.path, "chatos-target",
                "test ! -e \(quotedPath) && test ! -L \(quotedPath)",
            ],
            environment: runtime.environment,
            input: nil,
            timeout: 20,
            maximumCapturedBytes: 64 * 1_024
        )
        guard captured.exitCode == 0, !captured.timedOut else {
            throw NativeOpenSSHError.remoteFileExists(path)
        }
    }

    private static func runSFTPTransfer(arguments: [String], runtime: SSHRuntime) throws {
        // Modern OpenSSH scp uses the SFTP protocol by default while preserving
        // reliable exit codes and SSH_ASKPASS support for password connections.
        let captured = try run(
            executable: "/usr/bin/scp",
            arguments: ["-q", "-F", runtime.config.path] + arguments,
            environment: runtime.environment,
            input: nil,
            timeout: 60 * 60,
            maximumCapturedBytes: 512 * 1_024
        )
        guard captured.exitCode == 0, !captured.timedOut else {
            throw NativeOpenSSHError.remoteFailure(failureMessage(captured))
        }
    }

    private static func withRuntime<T>(
        draft: RemoteConnectionDraft,
        operation: (SSHRuntime) throws -> T
    ) throws -> T {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-remote-runtime-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        defer { try? FileManager.default.removeItem(at: directory) }
        let config = directory.appendingPathComponent("ssh_config")
        let askpass = directory.appendingPathComponent("askpass.sh")
        try NativeSSHConnectionTester.sshConfig(for: draft).write(to: config, atomically: true, encoding: .utf8)
        try NativeSSHConnectionTester.askpassScript.write(to: askpass, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: askpass.path)
        var environment = ProcessInfo.processInfo.environment
        environment["SSH_ASKPASS"] = askpass.path
        environment["SSH_ASKPASS_REQUIRE"] = "force"
        environment["DISPLAY"] = environment["DISPLAY"] ?? "chatos:0"
        environment["CHATOS_SSH_PASSWORD"] = draft.password ?? ""
        environment["CHATOS_SSH_JUMP_PASSWORD"] = draft.jumpPassword ?? ""
        environment["CHATOS_SSH_JUMP_HOST"] = draft.jumpHost ?? ""
        environment["CHATOS_SSH_JUMP_USER"] = draft.jumpUsername ?? ""
        environment["CHATOS_SSH_VERIFICATION_CODE"] = ""
        return try operation(.init(config: config, environment: environment))
    }

    private static func run(
        executable: String,
        arguments: [String],
        environment: [String: String],
        input: Data?,
        timeout: TimeInterval,
        maximumCapturedBytes: Int
    ) throws -> CapturedProcessResult {
        let process = Process()
        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        let stdout = BoundedDataBuffer(maximumBytes: maximumCapturedBytes)
        let stderr = BoundedDataBuffer(maximumBytes: maximumCapturedBytes)
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments
        process.environment = environment
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe
        let inputPipe: Pipe?
        if input != nil {
            let pipe = Pipe()
            process.standardInput = pipe
            inputPipe = pipe
        } else {
            process.standardInput = FileHandle.nullDevice
            inputPipe = nil
        }
        stdoutPipe.fileHandleForReading.readabilityHandler = { handle in stdout.append(handle.availableData) }
        stderrPipe.fileHandleForReading.readabilityHandler = { handle in stderr.append(handle.availableData) }
        do {
            try process.run()
        } catch {
            throw NativeOpenSSHError.launchFailed(error.localizedDescription)
        }
        if let input, let inputPipe {
            inputPipe.fileHandleForWriting.write(input)
            try? inputPipe.fileHandleForWriting.close()
        }
        let deadline = Date().addingTimeInterval(timeout)
        while process.isRunning, Date() < deadline { Thread.sleep(forTimeInterval: 0.025) }
        let timedOut = process.isRunning
        if timedOut {
            process.terminate()
            process.waitUntilExit()
        }
        stdoutPipe.fileHandleForReading.readabilityHandler = nil
        stderrPipe.fileHandleForReading.readabilityHandler = nil
        stdout.append(stdoutPipe.fileHandleForReading.readDataToEndOfFile())
        stderr.append(stderrPipe.fileHandleForReading.readDataToEndOfFile())
        return .init(
            exitCode: Int(process.terminationStatus),
            stdout: stdout.data,
            stderr: stderr.data,
            timedOut: timedOut,
            outputDiscarded: stdout.discarded || stderr.discarded
        )
    }

    private static func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    private static func remoteParent(_ path: String) -> String {
        let value = path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        guard let slash = value.lastIndex(of: "/") else { return "." }
        let parent = String(value[..<slash])
        return path.hasPrefix("/") ? "/" + parent : parent
    }

    private static func joinRemotePath(_ directory: String, _ name: String) -> String {
        if directory == "/" { return "/" + name }
        return directory.hasSuffix("/") ? directory + name : directory + "/" + name
    }

    private static func truncate(_ value: String, maximum: Int) -> (text: String, truncated: Bool) {
        guard value.count > maximum else { return (value, false) }
        return (String(value.suffix(maximum)), true)
    }

    private static func failureMessage(_ result: CapturedProcessResult) -> String {
        if result.timedOut { return "远程操作超时" }
        let stderr = String(decoding: result.stderr, as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return stderr.isEmpty ? "远程操作失败（退出码 \(result.exitCode)）" : stderr
    }
}

private struct SSHRuntime {
    let config: URL
    let environment: [String: String]
}

private struct CapturedProcessResult {
    let exitCode: Int
    let stdout: Data
    let stderr: Data
    let timedOut: Bool
    let outputDiscarded: Bool
}

private final class BoundedDataBuffer: @unchecked Sendable {
    private let lock = NSLock()
    private let maximumBytes: Int
    private var storage = Data()
    private(set) var discarded = false

    init(maximumBytes: Int) { self.maximumBytes = max(1, maximumBytes) }

    func append(_ data: Data) {
        guard !data.isEmpty else { return }
        lock.lock()
        let remaining = max(0, maximumBytes - storage.count)
        storage.append(data.prefix(remaining))
        if data.count > remaining { discarded = true }
        lock.unlock()
    }

    var data: Data {
        lock.lock()
        let value = storage
        lock.unlock()
        return value
    }
}

private enum NativeOpenSSHError: LocalizedError {
    case launchFailed(String)
    case remoteFailure(String)
    case fileTooLarge(Int)
    case localFileUnavailable(String)
    case localFileExists(String)
    case remoteFileExists(String)

    var errorDescription: String? {
        switch self {
        case let .launchFailed(message): "无法启动本机 SSH：\(message)"
        case let .remoteFailure(message): message
        case let .fileTooLarge(limit): "远程文件超过读取限制（\(limit) 字节）"
        case let .localFileUnavailable(path): "无法读取本机文件：\(path)"
        case let .localFileExists(path): "本机已有同名文件：\(path)"
        case let .remoteFileExists(path): "远端已有同名文件：\(path)"
        }
    }
}
