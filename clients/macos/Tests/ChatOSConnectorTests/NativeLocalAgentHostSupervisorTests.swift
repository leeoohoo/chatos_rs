// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

@testable import ChatOSConnector
import Foundation
import Testing

@Suite("Native local Agent Host supervisor")
struct NativeLocalAgentHostSupervisorTests {
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
        #expect(states.contains(.restarting(accountID: "user-1", attempt: 1)))
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
}

private actor LaunchCounter {
    private(set) var value = 0
    func increment() { value += 1 }
}

private struct RestartingHostFixture: Sendable {
    let directory: URL
    let executable: URL
    let socketPath: String

    init(alwaysWait: Bool = false) throws {
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
            sys.exit(17)
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
