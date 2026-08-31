import ChatOSCore
import Foundation

protocol NativeRemoteConnectionTesting: Sendable {
    func test(
        draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult
}

struct NativeSSHConnectionTester: NativeRemoteConnectionTesting {
    private let timeout: TimeInterval

    init(timeout: TimeInterval = 15) {
        self.timeout = timeout
    }

    func test(
        draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        let timeout = timeout
        return try await Task.detached(priority: .userInitiated) {
            try Self.runTest(
                draft: draft,
                verificationCode: verificationCode,
                timeout: timeout
            )
        }.value
    }

    private static func runTest(
        draft: RemoteConnectionDraft,
        verificationCode: String?,
        timeout: TimeInterval
    ) throws -> RemoteConnectionTestResult {
        try validate(draft)

        let temporaryDirectory = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-ssh-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(
            at: temporaryDirectory,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        defer { try? FileManager.default.removeItem(at: temporaryDirectory) }

        let configURL = temporaryDirectory.appendingPathComponent("ssh_config")
        let askpassURL = temporaryDirectory.appendingPathComponent("askpass.sh")
        let promptLogURL = temporaryDirectory.appendingPathComponent("prompts.log")
        try sshConfig(for: draft).write(to: configURL, atomically: true, encoding: .utf8)
        try askpassScript.write(to: askpassURL, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: askpassURL.path)

        let process = Process()
        let stdout = Pipe()
        let stderr = Pipe()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/ssh")
        process.arguments = [
            "-F", configURL.path,
            "chatos-target",
            "printf '__CHATOS_REMOTE_OK__ '; hostname",
        ]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = stdout
        process.standardError = stderr
        var environment = ProcessInfo.processInfo.environment
        environment["SSH_ASKPASS"] = askpassURL.path
        environment["SSH_ASKPASS_REQUIRE"] = "force"
        environment["DISPLAY"] = environment["DISPLAY"] ?? "chatos:0"
        environment["CHATOS_SSH_PROMPT_LOG"] = promptLogURL.path
        environment["CHATOS_SSH_PASSWORD"] = draft.password ?? ""
        environment["CHATOS_SSH_JUMP_PASSWORD"] = draft.jumpPassword ?? ""
        environment["CHATOS_SSH_JUMP_HOST"] = draft.jumpHost ?? ""
        environment["CHATOS_SSH_JUMP_USER"] = draft.jumpUsername ?? ""
        environment["CHATOS_SSH_VERIFICATION_CODE"] = verificationCode ?? ""
        process.environment = environment

        do {
            try process.run()
        } catch {
            throw NativeRemoteConnectionError(
                "无法启动本机 SSH：\(error.localizedDescription)"
            )
        }

        let deadline = Date().addingTimeInterval(timeout)
        while process.isRunning, Date() < deadline {
            Thread.sleep(forTimeInterval: 0.05)
        }
        if process.isRunning {
            process.terminate()
            process.waitUntilExit()
            throw NativeRemoteConnectionError("SSH 连接超时，请检查主机地址、端口和网络。")
        }

        let output = String(
            decoding: stdout.fileHandleForReading.readDataToEndOfFile(),
            as: UTF8.self
        ).trimmingCharacters(in: .whitespacesAndNewlines)
        let errorOutput = String(
            decoding: stderr.fileHandleForReading.readDataToEndOfFile(),
            as: UTF8.self
        ).trimmingCharacters(in: .whitespacesAndNewlines)
        let prompts = (try? String(contentsOf: promptLogURL, encoding: .utf8)) ?? ""

        guard process.terminationStatus == 0, output.contains("__CHATOS_REMOTE_OK__") else {
            if verificationCode?.trimmedNonEmpty == nil,
               let prompt = verificationPrompt(from: prompts) {
                throw RemoteVerificationChallenge(prompt: prompt)
            }
            throw NativeRemoteConnectionError(
                userFacingFailure(stderr: errorOutput, prompts: prompts)
            )
        }

        return RemoteConnectionTestResult(
            success: true,
            message: "连接成功"
        )
    }

    private static func validate(_ draft: RemoteConnectionDraft) throws {
        guard draft.host.trimmedNonEmpty != nil else {
            throw NativeRemoteConnectionError("请输入远端主机地址。")
        }
        guard draft.username.trimmedNonEmpty != nil else {
            throw NativeRemoteConnectionError("请输入登录用户名。")
        }
        guard (1...65_535).contains(draft.port) else {
            throw NativeRemoteConnectionError("SSH 端口必须在 1 到 65535 之间。")
        }
        switch draft.authenticationType {
        case .password where draft.password?.trimmedNonEmpty == nil:
            throw NativeRemoteConnectionError("本机没有保存这条连接的登录密码，请重新编辑并保存。")
        case .privateKey where draft.privateKeyPath?.trimmedNonEmpty == nil:
            throw NativeRemoteConnectionError("本机没有保存这条连接的私钥路径，请重新编辑并保存。")
        case .privateKeyCertificate where draft.privateKeyPath?.trimmedNonEmpty == nil:
            throw NativeRemoteConnectionError("本机没有保存这条连接的私钥路径，请重新编辑并保存。")
        case .privateKeyCertificate where draft.certificatePath?.trimmedNonEmpty == nil:
            throw NativeRemoteConnectionError("本机没有保存这条连接的 SSH 证书路径，请重新编辑并保存。")
        default:
            break
        }
        if let privateKeyPath = draft.privateKeyPath?.trimmedNonEmpty,
           !FileManager.default.isReadableFile(atPath: privateKeyPath) {
            throw NativeRemoteConnectionError("无法读取私钥文件：\(privateKeyPath)")
        }
        if let certificatePath = draft.certificatePath?.trimmedNonEmpty,
           !FileManager.default.isReadableFile(atPath: certificatePath) {
            throw NativeRemoteConnectionError("无法读取 SSH 证书文件：\(certificatePath)")
        }
    }

    static func sshConfig(for draft: RemoteConnectionDraft) throws -> String {
        var blocks: [String] = []
        var target = commonHostBlock(
            alias: "chatos-target",
            host: draft.host,
            port: draft.port,
            username: draft.username,
            policy: draft.hostKeyPolicy
        )
        target.append(contentsOf: authenticationLines(
            type: draft.authenticationType,
            privateKeyPath: draft.privateKeyPath,
            certificatePath: draft.certificatePath
        ))
        if draft.jumpEnabled {
            guard let jumpHost = draft.jumpHost?.trimmedNonEmpty,
                  let jumpUsername = draft.jumpUsername?.trimmedNonEmpty else {
                throw NativeRemoteConnectionError("跳板机地址和用户名不能为空。")
            }
            target.append("  ProxyJump chatos-jump")
            var jump = commonHostBlock(
                alias: "chatos-jump",
                host: jumpHost,
                port: draft.jumpPort ?? 22,
                username: jumpUsername,
                policy: draft.hostKeyPolicy
            )
            let jumpType: RemoteAuthenticationType = draft.jumpPrivateKeyPath?.trimmedNonEmpty == nil
                ? .password
                : (draft.jumpCertificatePath?.trimmedNonEmpty == nil
                    ? .privateKey
                    : .privateKeyCertificate)
            jump.append(contentsOf: authenticationLines(
                type: jumpType,
                privateKeyPath: draft.jumpPrivateKeyPath,
                certificatePath: draft.jumpCertificatePath
            ))
            blocks.append(jump.joined(separator: "\n"))
        }
        blocks.append(target.joined(separator: "\n"))
        return blocks.joined(separator: "\n\n") + "\n"
    }

    private static func commonHostBlock(
        alias: String,
        host: String,
        port: Int,
        username: String,
        policy: RemoteHostKeyPolicy
    ) -> [String] {
        [
            "Host \(alias)",
            "  HostName \(sshConfigValue(host))",
            "  Port \(port)",
            "  User \(sshConfigValue(username))",
            "  ConnectTimeout 10",
            "  ConnectionAttempts 1",
            "  ServerAliveInterval 5",
            "  ServerAliveCountMax 1",
            "  StrictHostKeyChecking \(policy == .strict ? "yes" : "accept-new")",
            "  BatchMode no",
            "  RequestTTY no",
            "  LogLevel ERROR",
        ]
    }

    private static func authenticationLines(
        type: RemoteAuthenticationType,
        privateKeyPath: String?,
        certificatePath: String?
    ) -> [String] {
        switch type {
        case .password:
            return [
                "  PubkeyAuthentication no",
                "  PasswordAuthentication yes",
                "  KbdInteractiveAuthentication yes",
                "  PreferredAuthentications keyboard-interactive,password",
            ]
        case .privateKey, .privateKeyCertificate:
            var lines = [
                "  PubkeyAuthentication yes",
                "  PasswordAuthentication no",
                "  KbdInteractiveAuthentication yes",
                "  PreferredAuthentications publickey,keyboard-interactive",
                "  IdentitiesOnly yes",
            ]
            if let privateKeyPath = privateKeyPath?.trimmedNonEmpty {
                lines.append("  IdentityFile \(sshConfigValue(privateKeyPath))")
            }
            if type == .privateKeyCertificate,
               let certificatePath = certificatePath?.trimmedNonEmpty {
                lines.append("  CertificateFile \(sshConfigValue(certificatePath))")
            }
            return lines
        }
    }

    private static func sshConfigValue(_ value: String) -> String {
        let escaped = value
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
        return "\"\(escaped)\""
    }

    private static func verificationPrompt(from prompts: String) -> String? {
        prompts
            .split(whereSeparator: \Character.isNewline)
            .map(String.init)
            .first { prompt in
                let value = prompt.lowercased()
                return value.contains("verification")
                    || value.contains("one-time")
                    || value.contains("otp")
                    || value.contains("token")
                    || value.contains("code")
            }
    }

    private static func userFacingFailure(stderr: String, prompts: String) -> String {
        let source = stderr.trimmedNonEmpty ?? prompts.trimmedNonEmpty ?? "SSH 连接失败。"
        let lowercased = source.lowercased()
        if lowercased.contains("host key verification failed") {
            return "主机密钥校验失败。请确认服务器密钥，或选择“首次连接时接受新密钥”。"
        }
        if lowercased.contains("permission denied") {
            return "SSH 认证失败，请检查用户名和本机保存的密码或私钥。"
        }
        if lowercased.contains("connection refused") {
            return "远端服务器拒绝连接，请检查 SSH 端口和服务状态。"
        }
        if lowercased.contains("no route to host") || lowercased.contains("operation timed out") {
            return "无法访问远端主机，请检查网络、地址和防火墙。"
        }
        return source
    }

    static let askpassScript = """
    #!/bin/sh
    prompt="$1"
    if [ -n "$CHATOS_SSH_PROMPT_LOG" ]; then
      printf '%s\\n' "$prompt" >> "$CHATOS_SSH_PROMPT_LOG"
    fi
    lower="$(printf '%s' "$prompt" | tr '[:upper:]' '[:lower:]')"
    case "$lower" in
      *verification*|*one-time*|*otp*|*token*|*code*)
        printf '%s\\n' "$CHATOS_SSH_VERIFICATION_CODE"
        ;;
      *)
        if [ -n "$CHATOS_SSH_JUMP_HOST" ]; then
          case "$prompt" in
            *"$CHATOS_SSH_JUMP_HOST"*|*"$CHATOS_SSH_JUMP_USER"*)
              printf '%s\\n' "$CHATOS_SSH_JUMP_PASSWORD"
              exit 0
              ;;
          esac
        fi
        printf '%s\\n' "$CHATOS_SSH_PASSWORD"
        ;;
    esac
    """
}

private struct NativeRemoteConnectionError: LocalizedError {
    let message: String

    init(_ message: String) {
        self.message = message
    }

    var errorDescription: String? { message }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
