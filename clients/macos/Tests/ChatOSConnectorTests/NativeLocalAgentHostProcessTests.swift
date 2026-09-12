// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

@testable import ChatOSConnector
import Foundation
import Testing

@Suite("Native local Agent Host process")
struct NativeLocalAgentHostProcessTests {
    @Test("rejects an executable without the bundled Host identity")
    func rejectsWrongExecutableIdentity() async throws {
        let fixture = try HostFixture(mode: "ready")

        await #expect(throws: NativeLocalAgentHostLaunchError.untrustedExecutable) {
            _ = try await NativeLocalAgentHostProcessLauncher().launch(
                fixture.configuration()
            )
        }
    }

    @Test("uses opaque stdin references and accepts only the correlated ready frame")
    func performsCorrelatedHandshake() async throws {
        let fixture = try HostFixture(mode: "ready")
        let configuration = try fixture.configuration()

        let process = try await testHostLauncher().launch(configuration)

        #expect(process.isRunning)
        #expect(process.ready.launchID == "launch-1")
        #expect(process.ready.clientEndpoint == fixture.socketPath)
        #expect(await process.stop() != 0)
        #expect(!process.isRunning)
    }

    @Test("rejects a ready frame from the wrong launch")
    func rejectsMismatchedLaunch() async throws {
        let fixture = try HostFixture(mode: "wrong-launch")

        await #expect(throws: NativeLocalAgentHostLaunchError.readyLaunchMismatch) {
            _ = try await testHostLauncher().launch(
                fixture.configuration()
            )
        }
    }

    @Test("bounds a Host that never completes the ready handshake")
    func timesOutIncompleteReadyFrame() async throws {
        let fixture = try HostFixture(mode: "silent")

        await #expect(throws: NativeLocalAgentHostLaunchError.readyTimeout) {
            _ = try await testHostLauncher().launch(
                fixture.configuration(timeout: .milliseconds(100))
            )
        }
    }

    @Test("force-stops a Host that ignores graceful termination")
    func forceStopsUnresponsiveHost() async throws {
        let fixture = try HostFixture(mode: "ignore-term")
        let process = try await testHostLauncher().launch(
            fixture.configuration()
        )

        let status = await process.stop(gracePeriod: .milliseconds(50))

        #expect(status != 0)
        #expect(!process.isRunning)
    }

    @Test("captures bounded diagnostics when a Host exits before ready")
    func diagnosesEarlyExit() async throws {
        let fixture = try HostFixture(mode: "stderr-exit")

        do {
            _ = try await testHostLauncher().launch(
                fixture.configuration()
            )
            Issue.record("Expected the Host launch to fail")
        } catch let NativeLocalAgentHostLaunchError.processExitedBeforeReady(status, detail) {
            #expect(status == 42)
            #expect(detail.hasPrefix("startup failed:"))
            #expect(detail.utf8.count <= 2_048)
            #expect(!detail.contains("\n"))
        } catch {
            Issue.record("Unexpected launch error: \(error)")
        }
    }
}

private func testHostLauncher() -> NativeLocalAgentHostProcessLauncher {
    NativeLocalAgentHostProcessLauncher(testingIdentityVerifier: { _ in })
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
        secret_length = struct.unpack('>I', sys.stdin.buffer.read(4))[0]
        json.loads(sys.stdin.buffer.read(secret_length))
        if mode == 'stderr-exit':
            sys.stderr.write('startup failed:\\n' + ('x' * 4096))
            sys.stderr.flush()
            sys.exit(42)
        if mode == 'silent':
            time.sleep(30)
            sys.exit(0)
        ready = {
            'protocol_version': 4,
            'launch_id': 'wrong-launch' if mode == 'wrong-launch' else request['launch_id'],
            'process_id': os.getpid(),
            'client_endpoint': request['ipc_endpoint']['path'],
        }
        body = json.dumps(ready, separators=(',', ':')).encode()
        sys.stdout.buffer.write(struct.pack('>I', len(body)) + body)
        sys.stdout.buffer.flush()
        if mode == 'ignore-term':
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
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
            readyTimeout: timeout
        )
    }
}
