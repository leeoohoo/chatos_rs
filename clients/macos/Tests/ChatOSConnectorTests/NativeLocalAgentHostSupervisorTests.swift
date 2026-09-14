// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

@testable import ChatOSConnector
import ChatOSCore
import Darwin
import Foundation
import Testing

@Suite("Native local Agent Host supervisor")
struct NativeLocalAgentHostSupervisorTests {
    @Test("a rebuilt supervisor attaches the same durable Host PID")
    func attachesExistingHostWithoutLaunchingAgain() async throws {
        let fixture = try PersistentHostFixture()
        defer { fixture.cleanup() }
        let verifier: @Sendable (pid_t) throws -> Void = { _ in }
        let first = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            attacher: NativeLocalAgentHostAttacher(testingIdentityVerifier: verifier),
            restartDelays: [.milliseconds(10)]
        )
        try await first.start(accountID: "user-1") {
            try fixture.configuration()
        }
        let firstState = await first.state()
        guard case let .running(_, firstProcessID, endpoint, _) = firstState else {
            Issue.record("Expected first Host to be running")
            return
        }
        #expect(endpoint == fixture.socketPath)
        await first.detach()

        let rebuilt = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            attacher: NativeLocalAgentHostAttacher(testingIdentityVerifier: verifier),
            restartDelays: [.milliseconds(10)]
        )
        try await rebuilt.start(accountID: "user-1") {
            try fixture.configuration()
        }
        guard case let .running(_, attachedProcessID, attachedEndpoint, _) = await rebuilt.state()
        else {
            Issue.record("Expected rebuilt supervisor to attach the Host")
            return
        }

        #expect(attachedProcessID == firstProcessID)
        #expect(attachedEndpoint == endpoint)
        #expect(fixture.launchCount() == 1)
        await rebuilt.logout()
    }

    @Test("an attached Host crash is recovered with a fresh process")
    func restartsAfterAttachedHostCrash() async throws {
        let fixture = try PersistentHostFixture()
        defer { fixture.cleanup() }
        let verifier: @Sendable (pid_t) throws -> Void = { _ in }
        let first = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            attacher: NativeLocalAgentHostAttacher(testingIdentityVerifier: verifier),
            restartDelays: [.milliseconds(10)]
        )
        try await first.start(accountID: "user-1") { try fixture.configuration() }
        guard case let .running(_, originalProcessID, _, _) = await first.state() else {
            Issue.record("Expected original Host to be running")
            return
        }
        await first.detach()

        let rebuilt = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            attacher: NativeLocalAgentHostAttacher(testingIdentityVerifier: verifier),
            restartDelays: [.milliseconds(10)]
        )
        try await rebuilt.start(accountID: "user-1") { try fixture.configuration() }
        #expect(Darwin.kill(pid_t(originalProcessID), SIGKILL) == 0)

        try await waitUntil(timeout: .seconds(15)) {
            guard case let .running(_, processID, _, restartCount) = await rebuilt.state()
            else { return false }
            return processID != originalProcessID && restartCount == 1
        }

        #expect(fixture.launchCount() == 2)
        await rebuilt.logout()
    }

    @Test("removes a private stale socket before launching the Host")
    func recoversStalePrivateSocket() async throws {
        let fixture = try PersistentHostFixture()
        defer { fixture.cleanup() }
        try createStaleUnixSocket(at: fixture.socketPath)
        let verifier: @Sendable (pid_t) throws -> Void = { _ in }
        let supervisor = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            attacher: NativeLocalAgentHostAttacher(testingIdentityVerifier: verifier),
            restartDelays: [.milliseconds(10)]
        )

        try await supervisor.start(accountID: "user-1") {
            try fixture.configuration()
        }

        #expect(fixture.launchCount() == 1)
        guard case .running = await supervisor.state() else {
            Issue.record("Expected Host to start after stale socket cleanup")
            return
        }
        await supervisor.logout()
    }

    @Test("does not replace a live endpoint with an untrusted peer")
    func rejectsUntrustedAttachedPeer() async throws {
        let fixture = try PersistentHostFixture()
        defer { fixture.cleanup() }
        let first = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            attacher: NativeLocalAgentHostAttacher(testingIdentityVerifier: { _ in }),
            restartDelays: [.milliseconds(10)]
        )
        try await first.start(accountID: "user-1") {
            try fixture.configuration()
        }
        let rejecting = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            attacher: NativeLocalAgentHostAttacher(testingIdentityVerifier: { _ in
                throw NativeLocalAgentIPCError.serverIdentityMismatch
            }),
            restartDelays: [.milliseconds(10)]
        )

        await #expect(throws: NativeLocalAgentIPCError.serverIdentityMismatch) {
            try await rejecting.start(accountID: "user-1") {
                try fixture.configuration()
            }
        }

        #expect(fixture.launchCount() == 1)
        #expect(FileManager.default.fileExists(atPath: fixture.socketPath))
        await first.logout()
    }

    @Test("does not attach a live Host owned by another account")
    func rejectsWrongOwnerHost() async throws {
        let fixture = try PersistentHostFixture(expectedOwner: "user-1")
        defer { fixture.cleanup() }
        let verifier: @Sendable (pid_t) throws -> Void = { _ in }
        let first = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            attacher: NativeLocalAgentHostAttacher(testingIdentityVerifier: verifier),
            restartDelays: [.milliseconds(10)]
        )
        try await first.start(accountID: "user-1") { try fixture.configuration() }
        let second = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            attacher: NativeLocalAgentHostAttacher(testingIdentityVerifier: verifier),
            restartDelays: [.milliseconds(10)]
        )

        do {
            try await second.start(accountID: "user-2") { try fixture.configuration() }
            Issue.record("Expected owner-scoped attachment to fail")
        } catch let NativeLocalAgentIPCError.rejected(error) {
            #expect(error.code == "owner_scope_mismatch")
        } catch {
            Issue.record("Unexpected attachment error: \(error)")
        }

        #expect(fixture.launchCount() == 1)
        await first.logout()
    }

    @Test("replaces a trusted Host left behind by an older App protocol")
    func replacesTrustedOlderProtocolHost() async throws {
        let fixture = try PersistentHostFixture(responseProtocolVersion: 15)
        defer { fixture.cleanup() }
        let verifier: @Sendable (pid_t) throws -> Void = { _ in }
        let first = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            attacher: NativeLocalAgentHostAttacher(testingIdentityVerifier: verifier),
            restartDelays: [.milliseconds(10)]
        )
        try await first.start(accountID: "user-1") { try fixture.configuration() }
        guard case let .running(_, oldProcessID, _, _) = await first.state() else {
            Issue.record("Expected the old Host to be running")
            return
        }
        await first.detach()
        let second = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            attacher: NativeLocalAgentHostAttacher(testingIdentityVerifier: verifier),
            restartDelays: [.milliseconds(10)]
        )

        try await second.start(accountID: "user-1") { try fixture.configuration() }

        guard case let .running(_, newProcessID, _, _) = await second.state() else {
            Issue.record("Expected a replacement Host to be running")
            return
        }
        #expect(newProcessID != oldProcessID)
        #expect(fixture.launchCount() == 2)
        await second.logout()
    }

    @Test("restarts a crashed Host with a freshly produced launch frame")
    func restartsAfterCrash() async throws {
        let fixture = try RestartingHostFixture()
        let supervisor = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            restartDelays: [.milliseconds(10)]
        )
        let launches = LaunchCounter()
        let stateStream = await supervisor.stateUpdates()
        let observedStates = Task { () -> [NativeLocalAgentHostState] in
            var states: [NativeLocalAgentHostState] = []
            for await state in stateStream {
                states.append(state)
                if case let .running(_, _, _, restartCount) = state, restartCount == 1 {
                    return states
                }
            }
            return states
        }

        try await supervisor.start(accountID: "user-1") {
            await launches.increment()
            return try fixture.configuration()
        }

        try await waitUntil {
            if case let .running(_, _, _, restartCount) = await supervisor.state() {
                return restartCount == 1
            }
            return false
        }
        #expect(await launches.value == 2)
        let states = await observedStates.value
        #expect(states.contains(.starting(accountID: "user-1")))
        #expect(states.contains(.restarting(
            accountID: "user-1",
            attempt: 1,
            cause: .unexpected(status: 17)
        )))
        #expect(states.contains(where: {
            if case let .running(accountID, _, _, restartCount) = $0 {
                return accountID == "user-1" && restartCount == 1
            }
            return false
        }))
        await supervisor.logout()
        #expect(await supervisor.state() == .stopped)
    }

    @Test("logout prevents an intentional termination from restarting")
    func logoutStopsWithoutRestart() async throws {
        let fixture = try RestartingHostFixture(alwaysWait: true)
        let supervisor = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            restartDelays: [.zero]
        )
        let launches = LaunchCounter()
        try await supervisor.start(accountID: "user-1") {
            await launches.increment()
            return try fixture.configuration()
        }

        await supervisor.logout()
        try await Task.sleep(for: .milliseconds(100))

        #expect(await supervisor.state() == .stopped)
        #expect(await launches.value == 1)
    }

    @Test("publishes storage unavailability while the Host is restarting")
    func exposesStorageUnavailableDuringRestart() async throws {
        let fixture = try RestartingHostFixture(firstExitStatus: 75)
        let supervisor = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            restartDelays: [.milliseconds(250)]
        )

        try await supervisor.start(accountID: "user-1") {
            try fixture.configuration()
        }
        try await waitUntil {
            await supervisor.state() == .restarting(
                accountID: "user-1",
                attempt: 1,
                cause: .storageUnavailable
            )
        }

        await supervisor.logout()
    }

    @Test("preserves redacted crash diagnostics when every restart launch fails")
    func reportsPostReadyCrashDetail() async throws {
        let fixture = try RestartingHostFixture()
        let supervisor = try NativeLocalAgentHostSupervisor(
            launcher: NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in }),
            restartDelays: [.zero]
        )
        let launches = LaunchCounter()

        try await supervisor.start(accountID: "user-1") {
            let attempt = await launches.increment()
            guard attempt == 1 else {
                throw NativeLocalAgentHostLaunchError.processLaunchFailed("restart refused")
            }
            return try fixture.configuration()
        }
        try await waitUntil {
            if case .failed = await supervisor.state() { return true }
            return false
        }

        guard case let .failed(_, reason) = await supervisor.state() else {
            Issue.record("Expected the Host supervisor to publish a terminal failure")
            return
        }
        #expect(reason.contains("worker crashed"))
        #expect(reason.contains("[REDACTED]"))
        #expect(reason.contains("restart refused"))
        #expect(!reason.contains("secret-token"))
        #expect(!reason.contains("gateway.example.test"))
    }
}

private struct PersistentHostFixture: Sendable {
    let directory: URL
    let executable: URL
    let socketPath: String
    private let counterPath: String

    init(
        expectedOwner: String = "user-1",
        responseProtocolVersion: UInt32 = localAgentProtocolVersion
    ) throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700]
        )
        executable = directory.appendingPathComponent("persistent-host.py")
        socketPath = directory.appendingPathComponent("agent.sock").path
        counterPath = directory.appendingPathComponent("launch-count").path
        let script = """
        #!/usr/bin/python3
        import json, os, signal, socket, struct, sys
        socket_path = \(String(reflecting: socketPath))
        counter_path = \(String(reflecting: counterPath))
        count = 0
        if os.path.exists(counter_path):
            count = int(open(counter_path).read())
        open(counter_path, 'w').write(str(count + 1))
        def read_exact(stream, count):
            value = b''
            while len(value) < count:
                chunk = stream.read(count - len(value))
                if not chunk:
                    raise EOFError()
                value += chunk
            return value
        length = struct.unpack('>I', read_exact(sys.stdin.buffer, 4))[0]
        request = json.loads(read_exact(sys.stdin.buffer, length))
        secret_length = struct.unpack('>I', read_exact(sys.stdin.buffer, 4))[0]
        json.loads(read_exact(sys.stdin.buffer, secret_length))
        server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        server.bind(socket_path)
        os.chmod(socket_path, 0o600)
        server.listen(8)
        ready = {
            'protocol_version': 4,
            'launch_id': request['launch_id'],
            'process_id': os.getpid(),
            'client_endpoint': socket_path,
        }
        body = json.dumps(ready, separators=(',', ':')).encode()
        sys.stdout.buffer.write(struct.pack('>I', len(body)) + body)
        sys.stdout.buffer.flush()
        def stop(_signum, _frame):
            server.close()
            try:
                os.unlink(socket_path)
            except FileNotFoundError:
                pass
            sys.exit(0)
        signal.signal(signal.SIGTERM, stop)
        while True:
            connection, _ = server.accept()
            try:
                header = connection.recv(4)
                if len(header) != 4:
                    continue
                request_length = struct.unpack('>I', header)[0]
                encoded = b''
                while len(encoded) < request_length:
                    chunk = connection.recv(request_length - len(encoded))
                    if not chunk:
                        break
                    encoded += chunk
                ipc_request = json.loads(encoded)
                if ipc_request.get('owner_user_id') != \(String(reflecting: expectedOwner)):
                    response = {
                        'type': 'error',
                        'payload': {
                            'code': 'owner_scope_mismatch',
                            'message': 'owner mismatch',
                            'retryable': False,
                        },
                    }
                else:
                    response = {
                        'type': 'ui_event_cursor',
                        'payload': {'event_seq': 0},
                    }
                reply = json.dumps({
                    'protocol_version': \(responseProtocolVersion),
                    'request_id': ipc_request['request_id'],
                    'response': response,
                }, separators=(',', ':')).encode()
                connection.sendall(struct.pack('>I', len(reply)) + reply)
            finally:
                connection.close()
        """
        try script.write(to: executable, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: executable.path
        )
    }

    func configuration() throws -> NativeLocalAgentHostLaunchConfiguration {
        let launchID = "launch-\(UUID().uuidString.lowercased())"
        let request = try JSONSerialization.data(withJSONObject: [
            "protocol_version": 4,
            "launch_id": launchID,
            "ipc_endpoint": ["transport": "unix_socket", "path": socketPath],
            "credential_references": [
                "model_access_token_reference": "model-access-token",
                "provider_context_key_reference": "provider-context-key",
            ],
        ])
        let secrets = try JSONSerialization.data(withJSONObject: [
            "protocol_version": 4,
            "launch_id": launchID,
            "secrets": [],
        ])
        return try NativeLocalAgentHostLaunchConfiguration(
            executableURL: executable,
            launchID: launchID,
            expectedClientEndpoint: socketPath,
            launchRequestJSON: request,
            secretFrameJSON: secrets,
            readyTimeout: .seconds(10)
        )
    }

    func launchCount() -> Int {
        guard let value = try? String(contentsOfFile: counterPath, encoding: .utf8) else {
            return 0
        }
        return Int(value) ?? 0
    }

    func cleanup() {
        try? FileManager.default.removeItem(at: directory)
    }
}

private func createStaleUnixSocket(at path: String) throws {
    let descriptor = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
    guard descriptor >= 0 else { throw NativeLocalAgentIPCError.socketUnavailable(errno) }
    defer { Darwin.close(descriptor) }
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let encodedPath = path.utf8CString
    withUnsafeMutableBytes(of: &address.sun_path) { destination in
        encodedPath.withUnsafeBytes { source in destination.copyBytes(from: source) }
    }
    let addressLength = socklen_t(MemoryLayout<sa_family_t>.size + encodedPath.count)
    address.sun_len = UInt8(addressLength)
    let result = withUnsafePointer(to: &address) { pointer in
        pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
            Darwin.bind(descriptor, $0, addressLength)
        }
    }
    guard result == 0 else { throw NativeLocalAgentIPCError.socketUnavailable(errno) }
    try FileManager.default.setAttributes(
        [.posixPermissions: 0o600],
        ofItemAtPath: path
    )
}

private actor LaunchCounter {
    private(set) var value = 0
    @discardableResult
    func increment() -> Int {
        value += 1
        return value
    }
}

private struct RestartingHostFixture: Sendable {
    let directory: URL
    let executable: URL
    let socketPath: String

    init(alwaysWait: Bool = false, firstExitStatus: Int32 = 17) throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700]
        )
        executable = directory.appendingPathComponent("host.py")
        socketPath = directory.appendingPathComponent("agent.sock").path
        let counter = directory.appendingPathComponent("launch-count").path
        let script = """
        #!/usr/bin/python3
        import json, os, signal, struct, sys
        counter_path = \(String(reflecting: counter))
        always_wait = \(alwaysWait ? "True" : "False")
        count = 0
        if os.path.exists(counter_path):
            count = int(open(counter_path).read())
        open(counter_path, 'w').write(str(count + 1))
        length = struct.unpack('>I', sys.stdin.buffer.read(4))[0]
        request = json.loads(sys.stdin.buffer.read(length))
        secret_length = struct.unpack('>I', sys.stdin.buffer.read(4))[0]
        json.loads(sys.stdin.buffer.read(secret_length))
        ready = {
            'protocol_version': 4,
            'launch_id': request['launch_id'],
            'process_id': os.getpid(),
            'client_endpoint': request['ipc_endpoint']['path'],
        }
        body = json.dumps(ready, separators=(',', ':')).encode()
        sys.stdout.buffer.write(struct.pack('>I', len(body)) + body)
        sys.stdout.buffer.flush()
        if not always_wait and count == 0:
            sys.stderr.write('worker crashed access_token=secret-token https://gateway.example.test/private\\n')
            sys.stderr.flush()
            sys.exit(\(firstExitStatus))
        signal.pause()
        """
        try script.write(to: executable, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: executable.path
        )
    }

    func configuration() throws -> NativeLocalAgentHostLaunchConfiguration {
        let request = try JSONSerialization.data(withJSONObject: [
            "protocol_version": 4,
            "launch_id": "launch-1",
            "ipc_endpoint": ["transport": "unix_socket", "path": socketPath],
            "credential_references": [
                "model_access_token_reference": "model-access-token",
                "provider_context_key_reference": "provider-context-key",
            ],
        ])
        let secrets = try JSONSerialization.data(withJSONObject: [
            "protocol_version": 4,
            "launch_id": "launch-1",
            "secrets": [],
        ])
        return try NativeLocalAgentHostLaunchConfiguration(
            executableURL: executable,
            launchID: "launch-1",
            expectedClientEndpoint: socketPath,
            launchRequestJSON: request,
            secretFrameJSON: secrets,
            readyTimeout: .seconds(15)
        )
    }
}

private func waitUntil(
    timeout: Duration = .seconds(10),
    condition: @escaping @Sendable () async -> Bool
) async throws {
    let clock = ContinuousClock()
    let deadline = clock.now.advanced(by: timeout)
    while clock.now < deadline {
        if await condition() { return }
        try await Task.sleep(for: .milliseconds(20))
    }
    Issue.record("Timed out waiting for local Agent Host state")
}
