import ChatOSCore
import ChatOSProcessRuntime
import CryptoKit
import Darwin
import Dispatch
import Foundation

protocol NativeRemoteSSHExecuting: Sendable {
    func hasReusableConnection(draft: RemoteConnectionDraft) async -> Bool

    func prepareAuthenticatedConnection(
        draft: RemoteConnectionDraft,
        verificationCode: String
    ) async throws

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
    /// SSH/SCP lifecycles synchronously wait on Process and FileHandle operations. Keep those
    /// waits on GCD-owned blocking lanes so multiple remote operations cannot exhaust Swift's
    /// cooperative executor and stall unrelated conversations. The queue is concurrent because
    /// each operation owns its Process, pipes and temporary runtime directory.
    private static let blockingWorkQueue = DispatchQueue(
        label: "com.chatos.open-ssh.blocking-work",
        qos: .userInitiated,
        attributes: .concurrent
    )
    private let remoteCommitAcknowledgementDelaySeconds: Int

    init(remoteCommitAcknowledgementDelaySeconds: Int = 0) {
        self.remoteCommitAcknowledgementDelaySeconds = max(
            0,
            remoteCommitAcknowledgementDelaySeconds
        )
    }

    func hasReusableConnection(draft: RemoteConnectionDraft) async -> Bool {
        (try? await Self.performBlockingWork { controller in
            (try? Self.withRuntime(draft: draft) { runtime in
                let captured = try Self.run(
                    executable: "/usr/bin/ssh",
                    arguments: ["-O", "check", "-F", runtime.config.path, "chatos-target"],
                    environment: runtime.environment,
                    input: nil,
                    timeout: 5,
                    maximumCapturedBytes: 64 * 1_024,
                    controller: controller
                )
                return captured.exitCode == 0 && !captured.timedOut
            }) ?? false
        }) ?? false
    }

    func prepareAuthenticatedConnection(
        draft: RemoteConnectionDraft,
        verificationCode: String
    ) async throws {
        try await Self.performBlockingWork { controller in
            try Self.withRuntime(
                draft: draft,
                verificationCode: verificationCode
            ) { runtime in
                let captured = try Self.run(
                    executable: "/usr/bin/ssh",
                    arguments: ["-F", runtime.config.path, "chatos-target", "true"],
                    environment: runtime.environment,
                    input: nil,
                    timeout: 30,
                    maximumCapturedBytes: 256 * 1_024,
                    controller: controller
                )
                guard captured.exitCode == 0, !captured.timedOut else {
                    throw NativeOpenSSHError.remoteFailure(Self.failureMessage(captured))
                }
                let controlPath = try Self.persistentControlPath(for: draft)
                guard FileManager.default.fileExists(atPath: controlPath.path) else {
                    throw NativeOpenSSHError.remoteFailure(
                        "SSH 已通过验证，但未能建立可复用连接。"
                    )
                }
            }
        }
    }

    func runCommand(
        draft: RemoteConnectionDraft,
        command: String,
        timeoutSeconds: Int,
        maximumOutputCharacters: Int
    ) async throws -> NativeRemoteCommandResult {
        try await Self.performBlockingWork { controller in
            try Self.withRuntime(draft: draft) { runtime in
                let captured = try Self.run(
                    executable: "/usr/bin/ssh",
                    arguments: ["-F", runtime.config.path, "chatos-target", command],
                    environment: runtime.environment,
                    input: nil,
                    timeout: TimeInterval(timeoutSeconds),
                    maximumCapturedBytes: max(maximumOutputCharacters * 4, 64 * 1_024),
                    controller: controller
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
        }
    }

    func listDirectory(
        draft: RemoteConnectionDraft,
        path: String,
        limit: Int
    ) async throws -> [NativeRemoteDirectoryEntry] {
        try await Self.performBlockingWork { controller in
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
                  metadata=$(stat -c '%s %Y %A' -- "$item" 2>/dev/null || stat -f '%z %m %Sp' "$item" 2>/dev/null || true)
                  size=${metadata%% *}
                  remainder=${metadata#* }
                  modified=${remainder%% *}
                  permissions=${remainder#* }
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
                    maximumCapturedBytes: 2 * 1_024 * 1_024,
                    controller: controller
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
        }
    }

    func resolveDirectory(
        draft: RemoteConnectionDraft,
        path: String
    ) async throws -> String {
        try await Self.performBlockingWork { controller in
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
                    maximumCapturedBytes: 64 * 1_024,
                    controller: controller
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
        }
    }

    func download(
        draft: RemoteConnectionDraft,
        path: String,
        maximumBytes: Int
    ) async throws -> Data {
        try await Self.performBlockingWork { controller in
            try Self.withRuntime(draft: draft) { runtime in
                let limit = min(max(maximumBytes, 1), 256 * 1_024)
                let command = "test -f \(Self.shellQuote(path)) && head -c \(limit + 1) -- \(Self.shellQuote(path))"
                let captured = try Self.run(
                    executable: "/usr/bin/ssh",
                    arguments: ["-F", runtime.config.path, "chatos-target", command],
                    environment: runtime.environment,
                    input: nil,
                    timeout: 20,
                    maximumCapturedBytes: limit + 1,
                    controller: controller
                )
                guard captured.exitCode == 0, !captured.timedOut else {
                    throw NativeOpenSSHError.remoteFailure(Self.failureMessage(captured))
                }
                guard captured.stdout.count <= limit, !captured.outputDiscarded else {
                    throw NativeOpenSSHError.fileTooLarge(limit)
                }
                return captured.stdout
            }
        }
    }

    func upload(
        draft: RemoteConnectionDraft,
        path: String,
        data: Data,
        createParentDirectories: Bool,
        overwrite: Bool
    ) async throws {
        try await Self.performBlockingWork { controller in
            try Self.withRuntime(draft: draft) { runtime in
                let quotedPath = Self.shellQuote(path)
                let parent = Self.shellQuote(Self.remoteParent(path))
                var commands = [
                    "set -e",
                    "target=\(quotedPath)",
                    "parent=\(parent)",
                ]
                if createParentDirectories { commands.append("mkdir -p -- \"$parent\"") }
                commands.append("tmp=$(mktemp \"$parent/.chatos-upload.XXXXXX\")")
                commands.append("cleanup() { rm -f -- \"$tmp\"; }")
                commands.append("trap cleanup EXIT HUP INT TERM")
                commands.append("cat > \"$tmp\"")
                if overwrite {
                    commands.append("if [ -e \"$target\" ] && [ ! -L \"$target\" ]; then mode=$(stat -c '%a' -- \"$target\" 2>/dev/null || stat -f '%Lp' \"$target\" 2>/dev/null || true); [ -z \"$mode\" ] || chmod \"$mode\" \"$tmp\"; fi")
                    commands.append("mv -f -- \"$tmp\" \"$target\"")
                } else {
                    commands.append("ln -- \"$tmp\" \"$target\" 2>/dev/null || { printf 'remote target already exists: %s\\n' \"$target\" >&2; exit 73; }")
                    commands.append("rm -f -- \"$tmp\"")
                }
                commands.append("trap - EXIT HUP INT TERM")
                let captured: CapturedProcessResult
                do {
                    captured = try Self.run(
                        executable: "/usr/bin/ssh",
                        arguments: [
                            "-F", runtime.config.path,
                            "chatos-target", commands.joined(separator: "; "),
                        ],
                        environment: runtime.environment,
                        input: data,
                        timeout: 30,
                        maximumCapturedBytes: 256 * 1_024,
                        controller: controller
                    )
                } catch {
                    if controller.isCancelled {
                        throw RemoteFileMutationFailure.reconciliationRequired(
                            "（上传确认前被取消）"
                        )
                    }
                    throw error
                }
                if !overwrite, captured.exitCode == 73, !captured.timedOut {
                    throw NativeOpenSSHError.remoteFileExists(path)
                }
                guard captured.exitCode == 0, !captured.timedOut else {
                    throw RemoteFileMutationFailure.reconciliationRequired(
                        Self.failureSuffix(captured)
                    )
                }
            }
        }
    }

    func uploadFile(
        draft: RemoteConnectionDraft,
        localURL: URL,
        remotePath: String,
        overwrite: Bool
    ) async throws {
        let acknowledgementDelay = remoteCommitAcknowledgementDelaySeconds
        try await Self.performBlockingWork { controller in
            guard FileManager.default.isReadableFile(atPath: localURL.path) else {
                throw NativeOpenSSHError.localFileUnavailable(localURL.path)
            }
            try Self.withRuntime(draft: draft) { runtime in
                let remoteParent = Self.remoteParent(remotePath)
                try Self.runRemotePreparation(
                    Self.remoteTransferCleanupScript(for: remoteParent),
                    runtime: runtime,
                    controller: controller
                )
                let temporaryRemotePath = Self.remoteTransferTemporaryPath(for: remotePath)
                var committed = false
                defer {
                    if !committed {
                        Self.removeRemoteTransferTemporaryFile(
                            temporaryRemotePath,
                            runtime: runtime
                        )
                    }
                }
                if !overwrite {
                    try Self.requireRemotePathMissing(
                        remotePath,
                        runtime: runtime,
                        controller: controller
                    )
                }
                try Self.runRemotePreparation(
                    "mkdir -p -- \(Self.shellQuote(remoteParent)); "
                        + "umask 077; : > \(Self.shellQuote(temporaryRemotePath))",
                    runtime: runtime,
                    controller: controller
                )
                try Self.runSFTPTransfer(
                    arguments: [localURL.path, "chatos-target:\(temporaryRemotePath)"],
                    runtime: runtime,
                    controller: controller
                )
                try controller.throwIfCancelled()
                try Self.commitRemoteTransfer(
                    temporaryPath: temporaryRemotePath,
                    destinationPath: remotePath,
                    overwrite: overwrite,
                    acknowledgementDelaySeconds: acknowledgementDelay,
                    runtime: runtime,
                    controller: controller
                )
                committed = true
            }
        }
    }

    func downloadFile(
        draft: RemoteConnectionDraft,
        remotePath: String,
        localURL: URL,
        overwrite: Bool
    ) async throws {
        try await Self.performBlockingWork { controller in
            if FileManager.default.fileExists(atPath: localURL.path), !overwrite {
                throw NativeOpenSSHError.localFileExists(localURL.path)
            }
            try FileManager.default.createDirectory(
                at: localURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            let temporaryURL = Self.localTransferTemporaryURL(for: localURL)
            defer { try? FileManager.default.removeItem(at: temporaryURL) }
            try Self.withRuntime(draft: draft) { runtime in
                try Self.runSFTPTransfer(
                    arguments: ["chatos-target:\(remotePath)", temporaryURL.path],
                    runtime: runtime,
                    controller: controller
                )
            }
            try controller.throwIfCancelled()
            try Self.commitDownloadedFile(
                temporaryURL: temporaryURL,
                destinationURL: localURL,
                overwrite: overwrite
            )
        }
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
        try await Self.performBlockingWork { controller in
            try Self.withRuntime(draft: draft) { runtime in
                try Self.runRemoteMutation(command, runtime: runtime, controller: controller)
            }
        }
    }

    static func performBlockingWork<T: Sendable>(
        _ operation: @escaping @Sendable (NativeOpenSSHProcessController) throws -> T
    ) async throws -> T {
        let controller = NativeOpenSSHProcessController()
        return try await withTaskCancellationHandler {
            do {
                let value: T = try await withCheckedThrowingContinuation { continuation in
                    blockingWorkQueue.async {
                        do {
                            try controller.throwIfCancelled()
                            continuation.resume(returning: try operation(controller))
                        } catch {
                            continuation.resume(throwing: error)
                        }
                    }
                }
                try Task.checkCancellation()
                return value
            } catch {
                if let failure = error as? RemoteFileMutationFailure,
                   failure.requiresReconciliation {
                    throw failure
                }
                if Task.isCancelled { throw CancellationError() }
                throw error
            }
        } onCancel: {
            controller.cancel()
        }
    }

    private static func runRemoteMutation(
        _ command: String,
        runtime: SSHRuntime,
        controller: NativeOpenSSHProcessController
    ) throws {
        let captured = try run(
            executable: "/usr/bin/ssh",
            arguments: ["-F", runtime.config.path, "chatos-target", "set -e; \(command)"],
            environment: runtime.environment,
            input: nil,
            timeout: 30,
            maximumCapturedBytes: 256 * 1_024,
            controller: controller
        )
        guard captured.exitCode == 0, !captured.timedOut else {
            throw RemoteFileMutationFailure.reconciliationRequired(failureSuffix(captured))
        }
    }

    private static func runRemotePreparation(
        _ command: String,
        runtime: SSHRuntime,
        controller: NativeOpenSSHProcessController
    ) throws {
        let captured = try run(
            executable: "/usr/bin/ssh",
            arguments: ["-F", runtime.config.path, "chatos-target", "set -e; \(command)"],
            environment: runtime.environment,
            input: nil,
            timeout: 30,
            maximumCapturedBytes: 256 * 1_024,
            controller: controller
        )
        guard captured.exitCode == 0, !captured.timedOut else {
            throw RemoteFileMutationFailure.notApplied(failureMessage(captured))
        }
    }

    private static func requireRemotePathMissing(
        _ path: String,
        runtime: SSHRuntime,
        controller: NativeOpenSSHProcessController
    ) throws {
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
            maximumCapturedBytes: 64 * 1_024,
            controller: controller
        )
        guard captured.exitCode == 0, !captured.timedOut else {
            throw NativeOpenSSHError.remoteFileExists(path)
        }
    }

    private static func runSFTPTransfer(
        arguments: [String],
        runtime: SSHRuntime,
        controller: NativeOpenSSHProcessController
    ) throws {
        // Modern OpenSSH scp uses the SFTP protocol by default while preserving
        // reliable exit codes and SSH_ASKPASS support for password connections.
        let captured = try run(
            executable: "/usr/bin/scp",
            arguments: ["-q", "-F", runtime.config.path] + arguments,
            environment: runtime.environment,
            input: nil,
            timeout: 60 * 60,
            maximumCapturedBytes: 512 * 1_024,
            controller: controller
        )
        guard captured.exitCode == 0, !captured.timedOut else {
            throw RemoteFileMutationFailure.notApplied(failureMessage(captured))
        }
    }

    private static func remoteTransferTemporaryPath(for destinationPath: String) -> String {
        joinRemotePath(
            remoteParent(destinationPath),
            ".chatos-upload-\(UUID().uuidString.lowercased()).tmp"
        )
    }

    static func remoteTransferCleanupScript(for parentPath: String) -> String {
        """
        parent=\(shellQuote(parentPath)); now=$(date +%s); \
        find "$parent" -mindepth 1 -maxdepth 1 -type f -name '.chatos-upload-*.tmp' -print 2>/dev/null | \
        while IFS= read -r item; do \
          name=${item##*/}; token=${name#.chatos-upload-}; token=${token%.tmp}; \
          [ ${#token} -eq 36 ] || continue; \
          case "$token" in ????????-????-????-????-????????????) ;; *) continue ;; esac; \
          case "$token" in *[!0-9a-f-]*) continue ;; esac; \
          links=$(stat -c '%h' -- "$item" 2>/dev/null || stat -f '%l' "$item" 2>/dev/null) || continue; \
          owner=$(stat -c '%u' -- "$item" 2>/dev/null || stat -f '%u' "$item" 2>/dev/null) || continue; \
          modified=$(stat -c '%Y' -- "$item" 2>/dev/null || stat -f '%m' "$item" 2>/dev/null) || continue; \
          [ "$links" = 1 ] && [ "$owner" = "$(id -u)" ] || continue; \
          [ $((now - modified)) -ge 86400 ] || continue; \
          rm -f -- "$item"; \
        done
        """
    }

    private static func commitRemoteTransfer(
        temporaryPath: String,
        destinationPath: String,
        overwrite: Bool,
        acknowledgementDelaySeconds: Int,
        runtime: SSHRuntime,
        controller: NativeOpenSSHProcessController
    ) throws {
        let quotedTemporaryPath = shellQuote(temporaryPath)
        let quotedDestinationPath = shellQuote(destinationPath)
        var commands = [
            "tmp=\(quotedTemporaryPath)",
            "target=\(quotedDestinationPath)",
            "test -f \"$tmp\" && test ! -L \"$tmp\"",
        ]
        if overwrite {
            commands.append("if [ -e \"$target\" ] && [ ! -L \"$target\" ]; then mode=$(stat -c '%a' -- \"$target\" 2>/dev/null || stat -f '%Lp' \"$target\" 2>/dev/null || true); [ -z \"$mode\" ] || chmod \"$mode\" \"$tmp\"; fi")
            commands.append("mv -f -- \"$tmp\" \"$target\"")
        } else {
            commands.append("ln -- \"$tmp\" \"$target\" 2>/dev/null || exit 73")
            commands.append("rm -f -- \"$tmp\"")
        }
        if acknowledgementDelaySeconds > 0 {
            commands.append("sleep \(acknowledgementDelaySeconds)")
        }
        let captured = try run(
            executable: "/usr/bin/ssh",
            arguments: [
                "-F", runtime.config.path, "chatos-target",
                "set -e; \(commands.joined(separator: "; "))",
            ],
            environment: runtime.environment,
            input: nil,
            timeout: 30,
            maximumCapturedBytes: 256 * 1_024,
            controller: controller
        )
        if !overwrite, captured.exitCode == 73, !captured.timedOut {
            throw NativeOpenSSHError.remoteFileExists(destinationPath)
        }
        guard captured.exitCode == 0, !captured.timedOut else {
            throw RemoteFileMutationFailure.reconciliationRequired(failureSuffix(captured))
        }
    }

    private static func removeRemoteTransferTemporaryFile(
        _ temporaryPath: String,
        runtime: SSHRuntime
    ) {
        let cleanupController = NativeOpenSSHProcessController()
        _ = try? run(
            executable: "/usr/bin/ssh",
            arguments: [
                "-F", runtime.config.path, "chatos-target",
                "rm -f -- \(shellQuote(temporaryPath))",
            ],
            environment: runtime.environment,
            input: nil,
            timeout: 5,
            maximumCapturedBytes: 64 * 1_024,
            controller: cleanupController
        )
    }

    private static func localTransferTemporaryURL(for destinationURL: URL) -> URL {
        destinationURL.deletingLastPathComponent().appendingPathComponent(
            ".chatos-download-\(UUID().uuidString.lowercased()).tmp",
            isDirectory: false
        )
    }

    static func commitDownloadedFile(
        temporaryURL: URL,
        destinationURL: URL,
        overwrite: Bool
    ) throws {
        let temporaryParent = temporaryURL.deletingLastPathComponent()
            .standardizedFileURL.resolvingSymlinksInPath()
        let destinationParent = destinationURL.deletingLastPathComponent()
            .standardizedFileURL.resolvingSymlinksInPath()
        guard temporaryParent == destinationParent,
              validLeafName(temporaryURL.lastPathComponent),
              validLeafName(destinationURL.lastPathComponent) else {
            throw NativeOpenSSHError.localFileCommitFailed(
                destinationURL.path,
                "临时文件必须位于目标目录的同一文件系统中"
            )
        }

        let directory = destinationParent.path.withCString {
            Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard directory >= 0 else {
            throw NativeOpenSSHError.localFileCommitFailed(
                destinationURL.path,
                String(cString: strerror(errno))
            )
        }
        defer { _ = Darwin.close(directory) }

        var directoryMetadata = stat()
        guard Darwin.fstat(directory, &directoryMetadata) == 0,
              (directoryMetadata.st_mode & S_IFMT) == S_IFDIR else {
            throw NativeOpenSSHError.localFileCommitFailed(
                destinationURL.path,
                "目标目录身份校验失败"
            )
        }
        let temporaryName = temporaryURL.lastPathComponent
        let destinationName = destinationURL.lastPathComponent
        let temporary = temporaryName.withCString {
            Darwin.openat(directory, $0, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard temporary >= 0 else {
            throw NativeOpenSSHError.localFileCommitFailed(
                destinationURL.path,
                String(cString: strerror(errno))
            )
        }
        defer { _ = Darwin.close(temporary) }

        var temporaryMetadata = stat()
        guard Darwin.fstat(temporary, &temporaryMetadata) == 0,
              (temporaryMetadata.st_mode & S_IFMT) == S_IFREG,
              temporaryMetadata.st_uid == geteuid(),
              temporaryMetadata.st_nlink == 1,
              temporaryMetadata.st_dev == directoryMetadata.st_dev else {
            throw NativeOpenSSHError.localFileCommitFailed(
                destinationURL.path,
                "临时文件身份或文件系统校验失败"
            )
        }

        var destinationMetadata = stat()
        let destinationStatus = destinationName.withCString {
            Darwin.fstatat(directory, $0, &destinationMetadata, AT_SYMLINK_NOFOLLOW)
        }
        if destinationStatus == 0 {
            guard overwrite else {
                throw NativeOpenSSHError.localFileExists(destinationURL.path)
            }
            guard (destinationMetadata.st_mode & S_IFMT) == S_IFREG else {
                throw NativeOpenSSHError.localFileCommitFailed(
                    destinationURL.path,
                    "目标不是可原子替换的普通文件"
                )
            }
            guard Darwin.fchmod(temporary, destinationMetadata.st_mode & 0o7777) == 0 else {
                throw NativeOpenSSHError.localFileCommitFailed(
                    destinationURL.path,
                    String(cString: strerror(errno))
                )
            }
        } else if errno != ENOENT {
            throw NativeOpenSSHError.localFileCommitFailed(
                destinationURL.path,
                String(cString: strerror(errno))
            )
        }

        guard Darwin.fsync(temporary) == 0 else {
            throw NativeOpenSSHError.localFileCommitFailed(
                destinationURL.path,
                String(cString: strerror(errno))
            )
        }
        let commitResult = temporaryName.withCString { source in
            destinationName.withCString { destination in
                overwrite
                    ? Darwin.renameat(directory, source, directory, destination)
                    : Darwin.linkat(directory, source, directory, destination, 0)
            }
        }
        guard commitResult == 0 else {
            let failure = errno
            if !overwrite, failure == EEXIST {
                throw NativeOpenSSHError.localFileExists(destinationURL.path)
            }
            throw NativeOpenSSHError.localFileCommitFailed(
                destinationURL.path,
                String(cString: strerror(failure))
            )
        }
        if !overwrite {
            let unlinkResult = temporaryName.withCString {
                Darwin.unlinkat(directory, $0, 0)
            }
            guard unlinkResult == 0 else {
                throw NativeOpenSSHError.localFileCommitUncertain(
                    destinationURL.path,
                    String(cString: strerror(errno))
                )
            }
        }
        guard Darwin.fsync(directory) == 0 else {
            throw NativeOpenSSHError.localFileCommitUncertain(
                destinationURL.path,
                String(cString: strerror(errno))
            )
        }
    }

    private static func validLeafName(_ name: String) -> Bool {
        !name.isEmpty && name != "." && name != ".."
            && !name.contains("/") && !name.contains("\0")
    }

    private static func withRuntime<T>(
        draft: RemoteConnectionDraft,
        verificationCode: String? = nil,
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
        let controlPath = try persistentControlPath(for: draft)
        let knownHostsURL = try NativeSSHConnectionTester.privateKnownHostsFile()
        try NativeSSHConnectionTester.sshConfig(
            for: draft,
            knownHostsPath: knownHostsURL.path,
            controlPath: controlPath.path
        )
            .write(to: config, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: config.path
        )
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
        environment["CHATOS_SSH_VERIFICATION_CODE"] = verificationCode ?? ""
        return try operation(.init(config: config, environment: environment))
    }

    static func persistentControlPath(for draft: RemoteConnectionDraft) throws -> URL {
        let identity = [
            draft.host,
            String(draft.port),
            draft.username,
            draft.authenticationType.rawValue,
            draft.password ?? "",
            draft.privateKeyPath ?? "",
            draft.certificatePath ?? "",
            draft.jumpHost ?? "",
            String(draft.jumpPort ?? 22),
            draft.jumpUsername ?? "",
            draft.jumpPassword ?? "",
            draft.jumpPrivateKeyPath ?? "",
            draft.jumpCertificatePath ?? "",
        ].joined(separator: "\u{1F}")
        let digest = SHA256.hash(data: Data(identity.utf8))
            .prefix(16)
            .map { String(format: "%02x", $0) }
            .joined()
        // macOS limits Unix-domain socket paths to roughly 104 bytes, and
        // OpenSSH appends a temporary suffix while creating the master socket.
        // NSTemporaryDirectory() is usually a long /var/folders/... path, so a
        // short per-user directory under /tmp is required for reliable reuse.
        let directory = URL(
            fileURLWithPath: "/tmp/chatos-ssh-\(getuid())",
            isDirectory: true
        )
        let fileManager = FileManager.default
        if !fileManager.fileExists(atPath: directory.path) {
            try fileManager.createDirectory(
                at: directory,
                withIntermediateDirectories: false,
                attributes: [.posixPermissions: 0o700]
            )
        }
        let attributes = try fileManager.attributesOfItem(atPath: directory.path)
        guard attributes[.type] as? FileAttributeType == .typeDirectory,
              (attributes[.ownerAccountID] as? NSNumber)?.uint32Value == getuid() else {
            throw NativeOpenSSHError.launchFailed("SSH 复用目录的归属或类型不安全")
        }
        try fileManager.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: directory.path
        )
        return directory.appendingPathComponent(digest, isDirectory: false)
    }

    static func run(
        executable: String,
        arguments: [String],
        environment: [String: String],
        input: Data?,
        timeout: TimeInterval,
        maximumCapturedBytes: Int,
        controller: NativeOpenSSHProcessController
    ) throws -> CapturedProcessResult {
        try controller.throwIfCancelled()
        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        let stdout = NativeBoundedProcessOutput(maximumBytes: maximumCapturedBytes)
        let stderr = NativeBoundedProcessOutput(maximumBytes: maximumCapturedBytes)
        let inputPipe: Pipe?
        let nullInput: Int32
        let standardInput: Int32
        if input != nil {
            let pipe = Pipe()
            inputPipe = pipe
            nullInput = -1
            standardInput = pipe.fileHandleForReading.fileDescriptor
        } else {
            inputPipe = nil
            nullInput = Darwin.open("/dev/null", O_RDONLY | O_CLOEXEC)
            guard nullInput >= 0 else {
                throw NativeOpenSSHError.launchFailed("无法打开 SSH 标准输入")
            }
            standardInput = nullInput
        }
        defer { if nullInput >= 0 { _ = Darwin.close(nullInput) } }
        NativeProcessPipeReader.install(
            on: stdoutPipe.fileHandleForReading,
            onData: stdout.append
        )
        NativeProcessPipeReader.install(
            on: stderrPipe.fileHandleForReading,
            onData: stderr.append
        )
        let processArguments = [executable] + arguments
        var processID: pid_t = 0
        let spawnResult = withCStringArray(processArguments) { argv in
            withCStringArray(environment.map { "\($0.key)=\($0.value)" }) { envp in
                chatos_spawn_process_group(
                    executable,
                    argv,
                    envp,
                    nil,
                    standardInput,
                    stdoutPipe.fileHandleForWriting.fileDescriptor,
                    stderrPipe.fileHandleForWriting.fileDescriptor,
                    &processID
                )
            }
        }
        inputPipe?.fileHandleForReading.closeFile()
        stdoutPipe.fileHandleForWriting.closeFile()
        stderrPipe.fileHandleForWriting.closeFile()
        guard spawnResult == 0, processID > 0 else {
            stdoutPipe.fileHandleForReading.readabilityHandler = nil
            stderrPipe.fileHandleForReading.readabilityHandler = nil
            throw NativeOpenSSHError.launchFailed(String(cString: strerror(spawnResult)))
        }
        let registration = controller.register(processID)
        defer { controller.clear(processID, registration: registration) }
        let exitSignal = NativeProcessExitSignal.reap(processID: processID)
        if let input, let inputPipe {
            do {
                try inputPipe.fileHandleForWriting.write(contentsOf: input)
            } catch {
                _ = chatos_signal_process_group(processID, SIGKILL)
                try? inputPipe.fileHandleForWriting.close()
                _ = exitSignal.wait(timeout: 2)
                throw NativeOpenSSHError.remoteFailure(error.localizedDescription)
            }
            try? inputPipe.fileHandleForWriting.close()
        }
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
        guard let exitCode else {
            stdoutPipe.fileHandleForReading.readabilityHandler = nil
            stderrPipe.fileHandleForReading.readabilityHandler = nil
            try? stdoutPipe.fileHandleForReading.close()
            try? stderrPipe.fileHandleForReading.close()
            throw NativeOpenSSHError.launchFailed("SSH 子进程无法在超时后终止")
        }
        stdoutPipe.fileHandleForReading.readabilityHandler = nil
        stderrPipe.fileHandleForReading.readabilityHandler = nil
        stdout.append(stdoutPipe.fileHandleForReading.readDataToEndOfFile())
        stderr.append(stderrPipe.fileHandleForReading.readDataToEndOfFile())
        let stdoutSnapshot = stdout.snapshot
        let stderrSnapshot = stderr.snapshot
        return .init(
            exitCode: Int(exitCode),
            stdout: stdoutSnapshot.data,
            stderr: stderrSnapshot.data,
            timedOut: timedOut,
            outputDiscarded: stdoutSnapshot.discarded || stderrSnapshot.discarded
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

    private static func shellQuote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    static func remoteParent(_ path: String) -> String {
        let isAbsolute = path.hasPrefix("/")
        let value = path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        guard let slash = value.lastIndex(of: "/") else {
            return isAbsolute ? "/" : "."
        }
        let parent = String(value[..<slash])
        guard !parent.isEmpty else { return isAbsolute ? "/" : "." }
        return isAbsolute ? "/" + parent : parent
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

    private static func failureSuffix(_ result: CapturedProcessResult) -> String {
        let message = failureMessage(result)
        return message.isEmpty ? "" : "（\(message)）"
    }
}

final class NativeOpenSSHProcessController: @unchecked Sendable {
    private static let forceKillQueue = DispatchQueue(
        label: "com.chatos.open-ssh.process-termination",
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
            guard let self,
                  self.owns(processID, registration: registration) else { return }
            _ = chatos_signal_process_group(processID, SIGKILL)
        }
    }

    private func owns(_ processID: pid_t, registration: UUID) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return activeProcessID == processID && activeRegistration == registration
    }
}

private struct SSHRuntime {
    let config: URL
    let environment: [String: String]
}

struct CapturedProcessResult {
    let exitCode: Int
    let stdout: Data
    let stderr: Data
    let timedOut: Bool
    let outputDiscarded: Bool
}

private enum NativeOpenSSHError: LocalizedError {
    case launchFailed(String)
    case remoteFailure(String)
    case fileTooLarge(Int)
    case localFileUnavailable(String)
    case localFileExists(String)
    case localFileCommitFailed(String, String)
    case localFileCommitUncertain(String, String)
    case remoteFileExists(String)

    var errorDescription: String? {
        switch self {
        case let .launchFailed(message): "无法启动本机 SSH：\(message)"
        case let .remoteFailure(message): message
        case let .fileTooLarge(limit): "远程文件超过读取限制（\(limit) 字节）"
        case let .localFileUnavailable(path): "无法读取本机文件：\(path)"
        case let .localFileExists(path): "本机已有同名文件：\(path)"
        case let .localFileCommitFailed(path, reason): "无法提交本机文件 \(path)：\(reason)"
        case let .localFileCommitUncertain(path, reason):
            "本机文件提交后的持久化结果尚未确认，请先核对 \(path) 后再重试：\(reason)"
        case let .remoteFileExists(path): "远端已有同名文件：\(path)"
        }
    }
}
