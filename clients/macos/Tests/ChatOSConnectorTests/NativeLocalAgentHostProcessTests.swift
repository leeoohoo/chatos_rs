// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSConnector
import Foundation
import Testing

@Suite("Native local Agent Host process")
struct NativeLocalAgentHostProcessTests {
    @Test("uses stdin credentials and accepts only the correlated ready frame")
    func performsCorrelatedHandshake() async throws {
        let fixture = try HostFixture(mode: "ready")
        let configuration = try fixture.configuration()

        let process = try await NativeLocalAgentHostProcessLauncher().launch(configuration)

        #expect(process.isRunning)
        #expect(process.ready.launchID == "launch-1")
        #expect(process.ready.clientEndpoint == fixture.socketPath)
        process.terminate()
        #expect(await process.waitForExit() != 0)
    }

    @Test("rejects a ready frame from the wrong launch")
    func rejectsMismatchedLaunch() async throws {
        let fixture = try HostFixture(mode: "wrong-launch")

        await #expect(throws: NativeLocalAgentHostLaunchError.readyLaunchMismatch) {
            _ = try await NativeLocalAgentHostProcessLauncher().launch(
                fixture.configuration()
            )
        }
    }

    @Test("bounds a Host that never completes the ready handshake")
    func timesOutIncompleteReadyFrame() async throws {
        let fixture = try HostFixture(mode: "silent")

        await #expect(throws: NativeLocalAgentHostLaunchError.readyTimeout) {
            _ = try await NativeLocalAgentHostProcessLauncher().launch(
                fixture.configuration(timeout: .milliseconds(100))
            )
        }
    }
}

private struct HostFixture {
    let directory: URL
    let executable: URL
    let socketPath: String

    init(mode: String) throws {
        directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700]
        )
        executable = directory.appendingPathComponent("host.py")
        socketPath = directory.appendingPathComponent("agent.sock").path
        let script = """
        #!/usr/bin/python3
        import json, os, signal, struct, sys, time
        mode = \(String(reflecting: mode))
        length = struct.unpack('>I', sys.stdin.buffer.read(4))[0]
        request = json.loads(sys.stdin.buffer.read(length))
        if mode == 'silent':
            time.sleep(30)
            sys.exit(0)
        ready = {
            'protocol_version': 2,
            'launch_id': 'wrong-launch' if mode == 'wrong-launch' else request['launch_id'],
            'process_id': os.getpid(),
            'client_endpoint': request['ipc_endpoint']['path'],
        }
        body = json.dumps(ready, separators=(',', ':')).encode()
        sys.stdout.buffer.write(struct.pack('>I', len(body)) + body)
        sys.stdout.buffer.flush()
        signal.pause()
        """
        try script.write(to: executable, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: executable.path
        )
    }

    func configuration(
        timeout: Duration = .seconds(15)
    ) throws -> NativeLocalAgentHostLaunchConfiguration {
        let request = try JSONSerialization.data(withJSONObject: [
            "protocol_version": 2,
            "launch_id": "launch-1",
            "ipc_endpoint": ["transport": "unix_socket", "path": socketPath],
            "credentials": ["model_access_token": "must-stay-on-stdin"],
        ])
        return try NativeLocalAgentHostLaunchConfiguration(
            executableURL: executable,
            launchID: "launch-1",
            expectedClientEndpoint: socketPath,
            launchRequestJSON: request,
            readyTimeout: timeout
        )
    }
}
