import ChatOSCore
import Foundation

protocol NativeRemoteConnectionTesting: Sendable {
    func test(
        draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult
}

struct NativeSSHConnectionTester: NativeRemoteConnectionTesting {
    private let coordinator: Coordinator

    init(timeout: TimeInterval = 15) {
        self.coordinator = Coordinator(timeout: timeout)
    }

    func test(
        draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        try await coordinator.test(draft: draft, verificationCode: verificationCode)
    }

    private actor Coordinator {
        private struct Session {
            var id: UUID
            var draft: RemoteConnectionDraft
            var process: Process
            var stdout: Pipe
            var stderrHandle: FileHandle
            var stderrURL: URL
            var temporaryDirectory: URL
            var promptLogURL: URL
            var verificationResponseURL: URL
        }

        private let timeout: TimeInterval
        private var sessions: [UUID: Session] = [:]

        init(timeout: TimeInterval) {
            self.timeout = timeout
        }

        func test(
            draft: RemoteConnectionDraft,
            verificationCode: String?
        ) async throws -> RemoteConnectionTestResult {
            try NativeSSHConnectionTester.validate(draft)
            let code = verificationCode?.trimmedNonEmpty

            if let code,
               let session = sessions.values.first(where: { $0.draft == draft }) {
                let promptCount = NativeSSHConnectionTester.verificationPrompts(
                    from: Self.readPrompts(from: session.promptLogURL)
                ).count
                try code.write(
                    to: session.verificationResponseURL,
                    atomically: true,
                    encoding: .utf8
                )
                try FileManager.default.setAttributes(
                    [.posixPermissions: 0o600],
                    ofItemAtPath: session.verificationResponseURL.path
                )
                return try await waitForCompletion(
                    sessionID: session.id,
                    verificationCodeWasSubmitted: true,
                    submittedPromptCount: promptCount
                )
            }

            discardSessions(matching: draft)
            let session = try startSession(draft: draft, verificationCode: code)
            sessions[session.id] = session
            return try await waitForCompletion(
                sessionID: session.id,
                verificationCodeWasSubmitted: code != nil,
                submittedPromptCount: nil
            )
        }

        private func startSession(
            draft: RemoteConnectionDraft,
            verificationCode: String?
        ) throws -> Session {
            let temporaryDirectory = FileManager.default.temporaryDirectory
                .appendingPathComponent("chatos-ssh-\(UUID().uuidString)", isDirectory: true)
            try FileManager.default.createDirectory(
                at: temporaryDirectory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            var sessionStarted = false
            defer {
                if !sessionStarted {
                    try? FileManager.default.removeItem(at: temporaryDirectory)
                }
            }

            let configURL = temporaryDirectory.appendingPathComponent("ssh_config")
            let askpassURL = temporaryDirectory.appendingPathComponent("askpass.sh")
            let promptLogURL = temporaryDirectory.appendingPathComponent("prompts.log")
            let stderrURL = temporaryDirectory.appendingPathComponent("ssh-stderr.log")
            let verificationResponseURL = temporaryDirectory
                .appendingPathComponent("verification-response")
            try NativeSSHConnectionTester.sshConfig(for: draft)
                .write(to: configURL, atomically: true, encoding: .utf8)
            try NativeSSHConnectionTester.askpassScript
                .write(to: askpassURL, atomically: true, encoding: .utf8)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o700],
                ofItemAtPath: askpassURL.path
            )

            let process = Process()
            let stdout = Pipe()
            guard FileManager.default.createFile(
                atPath: stderrURL.path,
                contents: nil,
                attributes: [.posixPermissions: 0o600]
            ) else {
                throw NativeRemoteConnectionError("无法创建 SSH 诊断日志。")
            }
            let stderrHandle = try FileHandle(forWritingTo: stderrURL)
            process.executableURL = URL(fileURLWithPath: "/usr/bin/ssh")
            process.arguments = [
                "-v",
                "-F", configURL.path,
                "chatos-target",
                "printf '__CHATOS_REMOTE_OK__ '; hostname",
            ]
            process.standardInput = FileHandle.nullDevice
            process.standardOutput = stdout
            process.standardError = stderrHandle
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
            environment["CHATOS_SSH_VERIFICATION_FILE"] = verificationResponseURL.path
            process.environment = environment

            do {
                try process.run()
            } catch {
                try? stderrHandle.close()
                throw NativeRemoteConnectionError(
                    "无法启动本机 SSH：\(error.localizedDescription)"
                )
            }

            sessionStarted = true
            return Session(
                id: UUID(),
                draft: draft,
                process: process,
                stdout: stdout,
                stderrHandle: stderrHandle,
                stderrURL: stderrURL,
                temporaryDirectory: temporaryDirectory,
                promptLogURL: promptLogURL,
                verificationResponseURL: verificationResponseURL
            )
        }

        private func waitForCompletion(
            sessionID: UUID,
            verificationCodeWasSubmitted: Bool,
            submittedPromptCount: Int?
        ) async throws -> RemoteConnectionTestResult {
            let deadline = Date().addingTimeInterval(timeout)
            while let session = sessions[sessionID],
                  session.process.isRunning,
                  Date() < deadline {
                let prompts = Self.readPrompts(from: session.promptLogURL)
                let diagnosticLog = Self.readText(from: session.stderrURL)
                if NativeSSHConnectionTester.diagnosticShowsAuthenticatedTarget(
                    diagnosticLog,
                    draft: session.draft
                ) {
                    discard(sessionID: sessionID)
                    return RemoteConnectionTestResult(success: true, message: "连接成功")
                }
                if !verificationCodeWasSubmitted,
                   let prompt = NativeSSHConnectionTester.verificationPrompt(from: prompts) {
                    scheduleExpiration(for: sessionID)
                    throw RemoteVerificationChallenge(prompt: prompt)
                }
                if let submittedPromptCount {
                    let verificationPrompts = NativeSSHConnectionTester
                        .verificationPrompts(from: prompts)
                    if verificationPrompts.count > submittedPromptCount,
                       let prompt = verificationPrompts.last {
                        scheduleExpiration(for: sessionID)
                        throw RemoteVerificationChallenge(prompt: prompt)
                    }
                }
                do {
                    try await Task.sleep(for: .milliseconds(50))
                } catch {
                    discard(sessionID: sessionID)
                    throw error
                }
            }

            guard let session = sessions[sessionID] else {
                throw NativeRemoteConnectionError("SSH 验证会话已失效，请重新测试连接。")
            }
            if session.process.isRunning {
                let prompts = Self.readPrompts(from: session.promptLogURL)
                discard(sessionID: sessionID)
                if prompts.trimmedNonEmpty != nil {
                    throw NativeRemoteConnectionError(
                        "已连接到 SSH 服务器，但认证等待超时。请确认使用最新短信验证码后重试。"
                    )
                }
                throw NativeRemoteConnectionError("SSH 连接超时，请检查主机地址、端口和网络。")
            }

            let output = String(
                decoding: session.stdout.fileHandleForReading.readDataToEndOfFile(),
                as: UTF8.self
            ).trimmingCharacters(in: .whitespacesAndNewlines)
            let errorOutput = Self.readText(from: session.stderrURL)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            let prompts = Self.readPrompts(from: session.promptLogURL)
            let terminationStatus = session.process.terminationStatus
            if NativeSSHConnectionTester.diagnosticShowsAuthenticatedTarget(
                errorOutput,
                draft: session.draft
            ) {
                discard(sessionID: sessionID, terminateIfRunning: false)
                return RemoteConnectionTestResult(success: true, message: "连接成功")
            }
            discard(sessionID: sessionID, terminateIfRunning: false)

            guard terminationStatus == 0, output.contains("__CHATOS_REMOTE_OK__") else {
                if !verificationCodeWasSubmitted,
                   let prompt = NativeSSHConnectionTester.verificationPrompt(from: prompts) {
                    throw RemoteVerificationChallenge(prompt: prompt)
                }
                throw NativeRemoteConnectionError(
                    NativeSSHConnectionTester.userFacingFailure(
                        stderr: errorOutput,
                        prompts: prompts
                    )
                )
            }

            return RemoteConnectionTestResult(success: true, message: "连接成功")
        }

        private static func readPrompts(from url: URL) -> String {
            readText(from: url)
        }

        private static func readText(from url: URL) -> String {
            (try? String(contentsOf: url, encoding: .utf8)) ?? ""
        }

        private func discardSessions(matching draft: RemoteConnectionDraft) {
            let matchingIDs = sessions.values
                .filter { $0.draft == draft }
                .map(\.id)
            for id in matchingIDs {
                discard(sessionID: id)
            }
        }

        private func discard(sessionID: UUID, terminateIfRunning: Bool = true) {
            guard let session = sessions.removeValue(forKey: sessionID) else { return }
            if terminateIfRunning, session.process.isRunning {
                session.process.terminate()
            }
            try? session.stderrHandle.close()
            try? FileManager.default.removeItem(at: session.temporaryDirectory)
        }

        private func scheduleExpiration(for sessionID: UUID) {
            Task { [weak self] in
                try? await Task.sleep(for: .seconds(5 * 60))
                await self?.discard(sessionID: sessionID)
            }
        }
    }

    static func validate(_ draft: RemoteConnectionDraft) throws {
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

    static func diagnosticShowsAuthenticatedTarget(
        _ diagnosticLog: String,
        draft: RemoteConnectionDraft
    ) -> Bool {
        guard let host = draft.host.trimmedNonEmpty?.lowercased() else { return false }
        let portSuffix = ":\(draft.port))"
        return diagnosticLog
            .split(whereSeparator: \Character.isNewline)
            .map { $0.lowercased() }
            .contains { line in
                line.contains("authenticated to \(host) ")
                    && line.contains(portSuffix)
            }
    }

    static func sshConfig(
        for draft: RemoteConnectionDraft,
        controlPath: String? = nil
    ) throws -> String {
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
        if let controlPath = controlPath?.trimmedNonEmpty {
            target.append("  ControlMaster auto")
            target.append("  ControlPersist 120")
            target.append("  ControlPath \(sshConfigValue(controlPath))")
        }
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

    private static func verificationPrompts(from prompts: String) -> [String] {
        prompts
            .split(whereSeparator: \Character.isNewline)
            .map(String.init)
            .filter { prompt in
                let value = prompt.lowercased()
                return value.contains("verification")
                    || value.contains("one-time")
                    || value.contains("otp")
                    || value.contains("mfa")
                    || value.contains("2fa")
                    || value.contains("sms")
                    || value.contains("token")
                    || value.contains("code")
                    || value.contains("验证码")
            }
    }

    private static func verificationPrompt(from prompts: String) -> String? {
        verificationPrompts(from: prompts).first
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
      *verification*|*one-time*|*otp*|*mfa*|*2fa*|*sms*|*token*|*code*|*验证码*)
        if [ -n "$CHATOS_SSH_VERIFICATION_CODE" ]; then
          printf '%s\\n' "$CHATOS_SSH_VERIFICATION_CODE"
          exit 0
        fi
        if [ -z "$CHATOS_SSH_VERIFICATION_FILE" ]; then
          printf '\\n'
          exit 0
        fi
        chatos_wait_count=0
        while [ "$chatos_wait_count" -lt 1200 ]; do
          if ! kill -0 "$PPID" 2>/dev/null; then
            exit 1
          fi
          if [ -s "$CHATOS_SSH_VERIFICATION_FILE" ]; then
            IFS= read -r chatos_verification_code < "$CHATOS_SSH_VERIFICATION_FILE"
            rm -f "$CHATOS_SSH_VERIFICATION_FILE"
            printf '%s\\n' "$chatos_verification_code"
            exit 0
          fi
          sleep 0.25
          chatos_wait_count=$((chatos_wait_count + 1))
        done
        printf '\\n'
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
