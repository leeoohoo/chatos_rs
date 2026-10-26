@testable import ChatOSConnector
import ChatOSCore
import Darwin
import Dispatch
import Foundation
import XCTest

final class NativeRemoteConnectionServiceTests: XCTestCase {
    func testStoresCredentialsLocallyAndNeverSendsThemToCloud() async throws {
        let upstream = RemoteConnectionUpstreamStub()
        let tester = RemoteConnectionTesterSpy()
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-remote-test-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let credentialStore = NativeRemoteConnectionCredentialStore(
            secretStore: NativeConnectorSecretStore(rootURL: root)
        )
        let connectorStateStore = NativeConnectorStateStore(
            stateURL: root.appendingPathComponent("connector-state.json")
        )
        var connectorState = NativeConnectorPersistentState.empty
        connectorState.deviceID = "device-current"
        connectorState.workspaces = [
            LocalConnectorWorkspace(
                id: "workspace-current",
                alias: "Mac",
                absoluteRoot: "/Users/test",
                fingerprint: "workspace-current"
            ),
        ]
        try connectorStateStore.save(connectorState)
        let service = NativeRemoteConnectionService(
            upstream: upstream,
            tester: tester,
            credentialStore: credentialStore,
            connectorStateStore: connectorStateStore
        )

        let created = try await service.createConnection(Self.passwordDraft)
        XCTAssertTrue(created.hasPassword)

        let capturedCreatedDraft = await upstream.lastCreatedDraft()
        let sentDraft = try XCTUnwrap(capturedCreatedDraft)
        XCTAssertNil(sentDraft.password)
        XCTAssertNil(sentDraft.privateKeyPath)
        XCTAssertEqual(
            sentDraft.localConnectorDeviceID,
            "device-current"
        )
        XCTAssertEqual(
            sentDraft.localConnectorWorkspaceID,
            "workspace-current"
        )

        _ = try await service.testSaved(id: created.id, verificationCode: nil)
        let capturedTestDraft = await tester.lastDraft()
        let testedDraft = try XCTUnwrap(capturedTestDraft)
        XCTAssertEqual(testedDraft.password, "local-secret")
        XCTAssertEqual(testedDraft.host, "server.example.com")
    }

    func testSSHConfigUsesNativeOpenSSHAndProxyJump() throws {
        var draft = Self.passwordDraft
        draft.jumpEnabled = true
        draft.jumpHost = "jump.example.com"
        draft.jumpPort = 2202
        draft.jumpUsername = "jump-user"
        draft.jumpPassword = "jump-secret"

        let config = try NativeSSHConnectionTester.sshConfig(
            for: draft,
            knownHostsPath: "/tmp/chatos-test-known-hosts"
        )

        XCTAssertTrue(config.contains("Host chatos-target"))
        XCTAssertTrue(config.contains("Host chatos-jump"))
        XCTAssertTrue(config.contains("ProxyJump chatos-jump"))
        XCTAssertTrue(config.contains("StrictHostKeyChecking accept-new"))
        XCTAssertTrue(config.contains("UserKnownHostsFile \"/tmp/chatos-test-known-hosts\""))
        XCTAssertTrue(config.contains("GlobalKnownHostsFile /dev/null"))
        XCTAssertFalse(config.contains("local_connector"))
        XCTAssertFalse(config.contains("local-secret"))
        XCTAssertFalse(config.contains("jump-secret"))
    }

    func testResolvedDraftCachesDirectConnectionLookupForConsecutiveTools() async throws {
        let upstream = RemoteConnectionUpstreamStub()
        let saved = try await upstream.createConnection(Self.passwordDraft)
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-remote-cache-test-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let service = NativeRemoteConnectionService(
            upstream: upstream,
            tester: RemoteConnectionTesterSpy(),
            credentialStore: NativeRemoteConnectionCredentialStore(
                secretStore: NativeConnectorSecretStore(rootURL: root)
            ),
            connectionCacheTTL: 15
        )

        _ = try await service.resolvedDraft(id: saved.id)
        _ = try await service.resolvedDraft(id: saved.id)

        let getRequests = await upstream.getRequestCount()
        let listRequests = await upstream.listRequestCount()
        XCTAssertEqual(getRequests, 1)
        XCTAssertEqual(listRequests, 0)
    }

    func testDisabledConnectionCacheDoesNotRetainExpiredEntries() async throws {
        let upstream = RemoteConnectionUpstreamStub()
        let saved = try await upstream.createConnection(Self.passwordDraft)
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-remote-no-cache-test-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let service = NativeRemoteConnectionService(
            upstream: upstream,
            tester: RemoteConnectionTesterSpy(),
            credentialStore: NativeRemoteConnectionCredentialStore(
                secretStore: NativeConnectorSecretStore(rootURL: root)
            ),
            connectionCacheTTL: 0
        )

        _ = try await service.getConnection(id: saved.id)
        _ = try await service.getConnection(id: saved.id)

        let retained = await service.cachedConnectionCount()
        let getRequests = await upstream.getRequestCount()
        XCTAssertEqual(retained, 0)
        XCTAssertEqual(getRequests, 2)
    }

    func testLegacyRouteMigratesToCurrentConnectorIdentifiers() async throws {
        let upstream = RemoteConnectionUpstreamStub()
        var legacyDraft = Self.passwordDraft
        legacyDraft.localConnectorDeviceID = NativeRemoteConnectionService.nativeDeviceID
        legacyDraft.localConnectorWorkspaceID = NativeRemoteConnectionService.nativeWorkspaceID
        let saved = try await upstream.createConnection(legacyDraft)
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-remote-route-test-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let stateStore = NativeConnectorStateStore(
            stateURL: root.appendingPathComponent("connector-state.json")
        )
        var state = NativeConnectorPersistentState.empty
        state.deviceID = "device-current"
        state.workspaces = [
            .init(
                id: "workspace-current",
                alias: "Mac",
                absoluteRoot: "/",
                fingerprint: "workspace-current"
            ),
        ]
        try stateStore.save(state)
        let service = NativeRemoteConnectionService(
            upstream: upstream,
            tester: RemoteConnectionTesterSpy(),
            credentialStore: NativeRemoteConnectionCredentialStore(
                secretStore: NativeConnectorSecretStore(rootURL: root)
            ),
            connectorStateStore: stateStore
        )

        let loaded = try await service.getConnection(id: saved.id)
        let migrated = try XCTUnwrap(loaded)
        let updateRequests = await upstream.updateRequestCount()

        XCTAssertEqual(migrated.localConnectorDeviceID, "device-current")
        XCTAssertEqual(migrated.localConnectorWorkspaceID, "workspace-current")
        XCTAssertEqual(updateRequests, 1)
    }

    func testSSHConfigCanReuseAConnectionWithoutExposingTheControlPath() throws {
        let config = try NativeSSHConnectionTester.sshConfig(
            for: Self.passwordDraft,
            knownHostsPath: "/tmp/chatos-test-known-hosts",
            controlPath: "/tmp/chatos-control-test"
        )

        XCTAssertTrue(config.contains("ControlMaster auto"))
        XCTAssertTrue(config.contains("ControlPersist 120"))
        XCTAssertTrue(config.contains("ControlPath \"/tmp/chatos-control-test\""))
    }

    func testSSHConfigAllowsHumanMFATimeOnTargetWithoutRelaxingJumpTimeout() throws {
        var draft = Self.passwordDraft
        draft.jumpEnabled = true
        draft.jumpHost = "jump.example.com"
        draft.jumpUsername = "jump-user"
        draft.jumpPassword = "jump-secret"

        let config = try NativeSSHConnectionTester.sshConfig(
            for: draft,
            knownHostsPath: "/tmp/chatos-test-known-hosts",
            targetConnectTimeoutSeconds: 300
        )
        let targetBlock = try XCTUnwrap(
            config.components(separatedBy: "Host chatos-target").last
        )
        let jumpBlock = try XCTUnwrap(
            config.components(separatedBy: "Host chatos-target").first
        )

        XCTAssertTrue(targetBlock.contains("ConnectTimeout 300"))
        XCTAssertTrue(jumpBlock.contains("ConnectTimeout 10"))
        XCTAssertFalse(jumpBlock.contains("ConnectTimeout 300"))
    }

    func testPrivateKnownHostsFileIsRegularOwnedAndPermissionRestricted() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "chatos-known-hosts-\(UUID().uuidString)",
            isDirectory: true
        )
        defer { try? FileManager.default.removeItem(at: root) }

        let knownHosts = try NativeSSHConnectionTester.privateKnownHostsFile(
            rootDirectory: root
        )
        let directoryAttributes = try FileManager.default.attributesOfItem(atPath: root.path)
        let fileAttributes = try FileManager.default.attributesOfItem(atPath: knownHosts.path)

        XCTAssertEqual(knownHosts.lastPathComponent, "known_hosts")
        XCTAssertEqual(directoryAttributes[.type] as? FileAttributeType, .typeDirectory)
        XCTAssertEqual((directoryAttributes[.posixPermissions] as? NSNumber)?.intValue, 0o700)
        XCTAssertEqual(fileAttributes[.type] as? FileAttributeType, .typeRegular)
        XCTAssertEqual((fileAttributes[.posixPermissions] as? NSNumber)?.intValue, 0o600)
        XCTAssertEqual((fileAttributes[.referenceCount] as? NSNumber)?.intValue, 1)
    }

    func testMFAAskpassWaitsForCodeFromTheExistingSSHSession() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-askpass-test-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let scriptURL = root.appendingPathComponent("askpass.sh")
        let promptLogURL = root.appendingPathComponent("prompts.log")
        let responseURL = root.appendingPathComponent("verification-response")
        try NativeSSHConnectionTester.askpassScript.write(
            to: scriptURL,
            atomically: true,
            encoding: .utf8
        )
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o700],
            ofItemAtPath: scriptURL.path
        )

        let outputURL = root.appendingPathComponent("submitted-code")
        XCTAssertTrue(FileManager.default.createFile(atPath: outputURL.path, contents: nil))
        let output = try FileHandle(forWritingTo: outputURL)
        let process = Process()
        process.executableURL = scriptURL
        process.arguments = ["(user@bastion.example.com) Please Input Mfa Code (SMS):"]
        process.standardOutput = output
        var environment = ProcessInfo.processInfo.environment
        environment["CHATOS_SSH_PROMPT_LOG"] = promptLogURL.path
        environment["CHATOS_SSH_VERIFICATION_CODE"] = ""
        environment["CHATOS_SSH_VERIFICATION_FILE"] = responseURL.path
        process.environment = environment
        try process.run()
        output.closeFile()
        let processID = process.processIdentifier

        // Fresh executable scripts can take a few seconds to start while macOS
        // performs security inspection, especially during a concurrent build.
        let promptDeadline = Date().addingTimeInterval(5)
        while !FileManager.default.fileExists(atPath: promptLogURL.path),
              Date() < promptDeadline {
            try await Task.sleep(for: .milliseconds(20))
        }
        XCTAssertTrue(process.isRunning)
        XCTAssertTrue(FileManager.default.fileExists(atPath: promptLogURL.path))

        try "614207".write(to: responseURL, atomically: true, encoding: .utf8)
        let completionDeadline = Date().addingTimeInterval(2)
        while Self.processIsAlive(processID), Date() < completionDeadline {
            try await Task.sleep(for: .milliseconds(20))
        }
        if Self.processIsAlive(processID) {
            _ = Darwin.kill(processID, SIGTERM)
            let terminationDeadline = Date().addingTimeInterval(1)
            while Self.processIsAlive(processID), Date() < terminationDeadline {
                try await Task.sleep(for: .milliseconds(10))
            }
            if Self.processIsAlive(processID) {
                _ = Darwin.kill(processID, SIGKILL)
            }
        }

        let submittedCode = try String(contentsOf: outputURL, encoding: .utf8)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        XCTAssertFalse(Self.processIsAlive(processID))
        XCTAssertEqual(submittedCode, "614207")
    }

    func testSSHDiagnosticReadsOnlyBoundedTail() throws {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-ssh-log-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: url) }
        let prefix = Data(repeating: UInt8(ascii: "a"), count: 2 * 1_024 * 1_024)
        let marker = Data("AUTHENTICATED-TAIL".utf8)
        try (prefix + marker).write(to: url)

        let text = NativeSSHConnectionTester.readBoundedText(
            from: url,
            maxBytes: NativeSSHConnectionTester.diagnosticLogByteLimit
        )

        XCTAssertLessThanOrEqual(text.utf8.count, NativeSSHConnectionTester.diagnosticLogByteLimit)
        XCTAssertTrue(text.hasSuffix("AUTHENTICATED-TAIL"))
        XCTAssertFalse(text.hasPrefix(String(repeating: "a", count: 1_100_000)))
    }

    func testConnectionTestAcceptsOpenSSHAuthenticationBeforeBastionSessionExits() {
        var draft = Self.passwordDraft
        draft.host = "mwpxljyjsn-public.bastionhost.aliyuncs.com"
        draft.port = 60_022
        let diagnosticLog = """
        debug1: Authentication succeeded (keyboard-interactive).
        Authenticated to mwpxljyjsn-public.bastionhost.aliyuncs.com ([10.0.0.8]:60022) using "keyboard-interactive".
        debug1: Entering interactive session.
        """

        XCTAssertTrue(
            NativeSSHConnectionTester.diagnosticShowsAuthenticatedTarget(
                diagnosticLog,
                draft: draft
            )
        )
    }

    func testConnectionTestDoesNotMistakeJumpHostAuthenticationForTarget() {
        var draft = Self.passwordDraft
        draft.host = "target.example.com"
        draft.port = 22
        draft.jumpEnabled = true
        draft.jumpHost = "jump.example.com"
        let diagnosticLog = """
        Authenticated to jump.example.com ([10.0.0.7]:22) using "publickey".
        """

        XCTAssertFalse(
            NativeSSHConnectionTester.diagnosticShowsAuthenticatedTarget(
                diagnosticLog,
                draft: draft
            )
        )
    }

    func testPersistentSSHControlPathFitsMacOSUnixSocketLimit() throws {
        let path = try NativeOpenSSHClient.persistentControlPath(for: Self.passwordDraft).path

        XCTAssertTrue(path.hasPrefix("/tmp/chatos-ssh-"))
        XCTAssertLessThan(path.utf8.count + 16, 104)
    }

    func testRemoteParentKeepsRootedTransferInRootDirectory() {
        XCTAssertEqual(NativeOpenSSHClient.remoteParent("/result.bin"), "/")
        XCTAssertEqual(NativeOpenSSHClient.remoteParent("/srv/result.bin"), "/srv")
        XCTAssertEqual(NativeOpenSSHClient.remoteParent("result.bin"), ".")
        XCTAssertEqual(NativeOpenSSHClient.remoteParent("build/result.bin"), "build")
    }

    func testRemoteTransferCleanupScriptRemovesOnlyCanonicalStaleSingleLinkFiles() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "chatos-remote-orphans-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let prefix = ".chatos-upload-"
        let stale = root.appendingPathComponent(
            prefix + UUID().uuidString.lowercased() + ".tmp"
        )
        let fresh = root.appendingPathComponent(
            prefix + UUID().uuidString.lowercased() + ".tmp"
        )
        let malformed = root.appendingPathComponent(prefix + "not-a-uuid.tmp")
        let uppercase = root.appendingPathComponent(
            prefix + UUID().uuidString.uppercased() + ".tmp"
        )
        let hardLinkSource = root.appendingPathComponent("hard-link-source")
        let hardLink = root.appendingPathComponent(
            prefix + UUID().uuidString.lowercased() + ".tmp"
        )
        let symlinkSource = root.appendingPathComponent("symlink-source")
        let symlink = root.appendingPathComponent(
            prefix + UUID().uuidString.lowercased() + ".tmp"
        )
        for url in [stale, fresh, malformed, uppercase, hardLinkSource, symlinkSource] {
            try Data("temporary".utf8).write(to: url)
        }
        try FileManager.default.linkItem(at: hardLinkSource, to: hardLink)
        try FileManager.default.createSymbolicLink(at: symlink, withDestinationURL: symlinkSource)
        let old = Date(timeIntervalSinceNow: -(25 * 60 * 60))
        for url in [stale, malformed, uppercase, hardLink] {
            try FileManager.default.setAttributes(
                [.modificationDate: old],
                ofItemAtPath: url.path
            )
        }

        let result = try NativeOpenSSHClient.run(
            executable: "/bin/sh",
            arguments: [
                "-c",
                NativeOpenSSHClient.remoteTransferCleanupScript(for: root.path),
            ],
            environment: ProcessInfo.processInfo.environment,
            input: nil,
            timeout: 5,
            maximumCapturedBytes: 64 * 1_024,
            controller: NativeOpenSSHProcessController()
        )

        XCTAssertEqual(result.exitCode, 0)
        XCTAssertFalse(FileManager.default.fileExists(atPath: stale.path))
        for preserved in [fresh, malformed, uppercase, hardLink, symlink] {
            XCTAssertTrue(
                FileManager.default.fileExists(atPath: preserved.path),
                "Unexpectedly removed \(preserved.lastPathComponent)"
            )
        }
    }

    func testRemoteTransferCleanupScriptAcceptsNoMatchesUnderZsh() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "chatos-remote-empty-orphans-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let result = try NativeOpenSSHClient.run(
            executable: "/bin/zsh",
            arguments: [
                "-c",
                NativeOpenSSHClient.remoteTransferCleanupScript(for: root.path),
            ],
            environment: ProcessInfo.processInfo.environment,
            input: nil,
            timeout: 5,
            maximumCapturedBytes: 64 * 1_024,
            controller: NativeOpenSSHProcessController()
        )

        XCTAssertEqual(result.exitCode, 0, String(decoding: result.stderr, as: UTF8.self))
    }

    func testOpenSSHBlockingWorkUsesConcurrentLanes() async throws {
        let firstStarted = expectation(description: "first blocking operation started")
        let secondStarted = expectation(description: "second blocking operation started")
        let releaseFirst = DispatchSemaphore(value: 0)
        let first = Task {
            try await NativeOpenSSHClient.performBlockingWork { _ in
                firstStarted.fulfill()
                return releaseFirst.wait(timeout: .now() + 5) == .success
            }
        }

        await fulfillment(of: [firstStarted], timeout: 2)
        let second = Task {
            try await NativeOpenSSHClient.performBlockingWork { _ in
                secondStarted.fulfill()
                return 2
            }
        }

        await fulfillment(of: [secondStarted], timeout: 2)
        releaseFirst.signal()
        let firstResult = try await first.value
        let secondResult = try await second.value
        XCTAssertTrue(firstResult, "独立 SSH 操作不应被另一个阻塞生命周期串行化")
        XCTAssertEqual(secondResult, 2)
    }

    func testCancellingOpenSSHBlockingWorkTerminatesOnlyItsRegisteredProcessGroup() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "chatos-ssh-cancel-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let childPIDURL = root.appendingPathComponent("child.pid")
        let unrelated = Process()
        unrelated.executableURL = URL(fileURLWithPath: "/bin/sleep")
        unrelated.arguments = ["30"]
        try unrelated.run()
        defer {
            if unrelated.isRunning { unrelated.terminate() }
            unrelated.waitUntilExit()
        }
        let task = Task {
            try await NativeOpenSSHClient.performBlockingWork { controller in
                try NativeOpenSSHClient.run(
                    executable: "/bin/sh",
                    arguments: [
                        "-c",
                        "sleep 30 & child=$!; printf '%s' \"$child\" > \(childPIDURL.path); wait",
                    ],
                    environment: ProcessInfo.processInfo.environment,
                    input: nil,
                    timeout: 30,
                    maximumCapturedBytes: 64 * 1_024,
                    controller: controller
                )
            }
        }

        for _ in 0..<200 where !FileManager.default.fileExists(atPath: childPIDURL.path) {
            try await Task.sleep(for: .milliseconds(10))
        }
        let childPID = try XCTUnwrap(pid_t(
            String(contentsOf: childPIDURL, encoding: .utf8)
        ))
        let cancellationStarted = ContinuousClock.now
        task.cancel()
        do {
            _ = try await task.value
            XCTFail("被取消的 SSH blocking work 不应正常返回")
        } catch is CancellationError {
            // Expected: cancellation is mapped after the owned child exits.
        }
        XCTAssertLessThan(
            cancellationStarted.duration(to: .now),
            .seconds(3),
            "取消应终止所属进程组，而不是等待远程操作原始超时"
        )
        for _ in 0..<100 where Darwin.kill(childPID, 0) == 0 {
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTAssertEqual(Darwin.kill(childPID, 0), -1, "SSH 孙进程不能在取消后残留")
        XCTAssertTrue(unrelated.isRunning, "取消不能影响不属于该操作的进程")
    }

    func testSuspendedOpenSSHProcessGroupResumesWithoutBlockingIndependentWork() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "chatos-ssh-suspend-\(UUID().uuidString)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let processGroupURL = root.appendingPathComponent("process-group.pid")
        let suspendedTask = Task {
            try await NativeOpenSSHClient.performBlockingWork { controller in
                try NativeOpenSSHClient.run(
                    executable: "/bin/sh",
                    arguments: [
                        "-c",
                        "printf '%s' \"$$\" > \(processGroupURL.path); "
                            + "kill -STOP \"$$\"; printf resumed",
                    ],
                    environment: ProcessInfo.processInfo.environment,
                    input: nil,
                    timeout: 10,
                    maximumCapturedBytes: 64 * 1_024,
                    controller: controller
                )
            }
        }
        var processGroup: pid_t?
        for _ in 0..<200 {
            if let value = try? String(contentsOf: processGroupURL, encoding: .utf8),
               let parsed = pid_t(value) {
                processGroup = parsed
                break
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        let suspendedProcessGroup = try XCTUnwrap(processGroup)
        defer { _ = Darwin.kill(-suspendedProcessGroup, SIGCONT) }

        let independent = try await NativeOpenSSHClient.performBlockingWork { _ in 7 }
        XCTAssertEqual(independent, 7, "暂停的 SSH 生命周期不能占用其他操作的执行槽")
        XCTAssertEqual(Darwin.kill(-suspendedProcessGroup, SIGCONT), 0)
        let result = try await suspendedTask.value
        XCTAssertEqual(result.exitCode, 0)
        XCTAssertEqual(String(decoding: result.stdout, as: UTF8.self), "resumed")
    }

    func testDownloadedFileCommitAtomicallyReplacesExistingDestination() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-download-commit-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let destination = root.appendingPathComponent("result.bin")
        let temporary = root.appendingPathComponent(".transfer.tmp")
        try Data("old".utf8).write(to: destination)
        try Data("complete-new".utf8).write(to: temporary)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o640],
            ofItemAtPath: destination.path
        )

        try NativeOpenSSHClient.commitDownloadedFile(
            temporaryURL: temporary,
            destinationURL: destination,
            overwrite: true
        )

        XCTAssertEqual(try Data(contentsOf: destination), Data("complete-new".utf8))
        XCTAssertFalse(FileManager.default.fileExists(atPath: temporary.path))
        let attributes = try FileManager.default.attributesOfItem(atPath: destination.path)
        XCTAssertEqual((attributes[.posixPermissions] as? NSNumber)?.intValue, 0o640)
    }

    func testDownloadedFileCommitWithoutOverwriteRejectsDestinationRace() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-download-race-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let destination = root.appendingPathComponent("result.bin")
        let temporary = root.appendingPathComponent(".transfer.tmp")
        try Data("racing-writer".utf8).write(to: destination)
        try Data("download".utf8).write(to: temporary)

        XCTAssertThrowsError(
            try NativeOpenSSHClient.commitDownloadedFile(
                temporaryURL: temporary,
                destinationURL: destination,
                overwrite: false
            )
        )
        XCTAssertEqual(try Data(contentsOf: destination), Data("racing-writer".utf8))
        XCTAssertEqual(try Data(contentsOf: temporary), Data("download".utf8))
    }

    func testFailedDownloadedFileCommitPreservesExistingDestination() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-download-failure-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let destination = root.appendingPathComponent("result.bin")
        let missingTemporary = root.appendingPathComponent(".missing.tmp")
        try Data("keep-me".utf8).write(to: destination)

        XCTAssertThrowsError(
            try NativeOpenSSHClient.commitDownloadedFile(
                temporaryURL: missingTemporary,
                destinationURL: destination,
                overwrite: true
            )
        )
        XCTAssertEqual(try Data(contentsOf: destination), Data("keep-me".utf8))
    }

    func testDownloadedFileCommitRejectsStagingOutsideDestinationDirectory() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-download-cross-directory-\(UUID().uuidString)", isDirectory: true)
        let staging = root.appendingPathComponent("staging", isDirectory: true)
        let destinationDirectory = root.appendingPathComponent("destination", isDirectory: true)
        try FileManager.default.createDirectory(at: staging, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(
            at: destinationDirectory,
            withIntermediateDirectories: true
        )
        defer { try? FileManager.default.removeItem(at: root) }
        let temporary = staging.appendingPathComponent(".transfer.tmp")
        let destination = destinationDirectory.appendingPathComponent("result.bin")
        try Data("complete-new".utf8).write(to: temporary)
        try Data("keep-me".utf8).write(to: destination)

        XCTAssertThrowsError(
            try NativeOpenSSHClient.commitDownloadedFile(
                temporaryURL: temporary,
                destinationURL: destination,
                overwrite: true
            )
        )

        XCTAssertEqual(try Data(contentsOf: destination), Data("keep-me".utf8))
        XCTAssertEqual(try Data(contentsOf: temporary), Data("complete-new".utf8))
    }

    func testRemoteTerminalOutputKeepsVisibleTextAndUpdatesWorkingDirectory() {
        let marker = "__CHATOS_REMOTE_CWD_TEST__"
        let parsed = NativeRemoteConnectionService.parseTerminalOutput(
            "first line\nsecond line\n\(marker)/srv/project\n",
            marker: marker,
            fallbackDirectory: "/root"
        )

        XCTAssertEqual(parsed.output, "first line\nsecond line")
        XCTAssertEqual(parsed.workingDirectory, "/srv/project")
    }

    func testInteractiveRemoteTerminalUsesTTYAndCleansCredentialRuntime() throws {
        let session = try NativeRemoteTerminalSession(
            connectionID: "connection-1",
            draft: Self.passwordDraft
        )
        let runtimeDirectory = session.runtimeDirectoryURL

        XCTAssertTrue(FileManager.default.fileExists(atPath: runtimeDirectory.path))
        XCTAssertEqual(Array(session.launchArguments.prefix(2)), ["-tt", "-F"])
        XCTAssertTrue(session.launchArguments.contains("chatos-target"))
        XCTAssertFalse(session.launchArguments.joined().contains("local-secret"))
        XCTAssertTrue(
            session.launchArguments.last?.contains("cd -- '/srv/app'") == true
        )
        let configPath = try XCTUnwrap(
            session.launchArguments.dropFirst().first { $0.hasSuffix("/ssh_config") }
        )
        let config = try String(contentsOfFile: configPath, encoding: .utf8)
        XCTAssertTrue(config.contains("ControlMaster auto"))
        XCTAssertTrue(config.contains("ControlPersist 120"))
        XCTAssertTrue(config.contains("ControlPath \"/tmp/chatos-ssh-"))

        session.close()
        session.close()

        XCTAssertFalse(FileManager.default.fileExists(atPath: runtimeDirectory.path))
    }

    func testSubmittedOneTimeCodeIsNotConsumedByAProbeConnection() async throws {
        let upstream = RemoteConnectionUpstreamStub()
        let tester = RemoteConnectionTesterSpy()
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-remote-otp-test-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let service = NativeRemoteConnectionService(
            upstream: upstream,
            tester: tester,
            credentialStore: NativeRemoteConnectionCredentialStore(
                secretStore: NativeConnectorSecretStore(rootURL: root)
            )
        )
        let connection = try await service.createConnection(Self.passwordDraft)

        let session = try await service.makeRemoteTerminalSession(
            connectionID: connection.id,
            verificationCode: "123456"
        )
        session.close()

        let requestCount = await tester.requestCount()
        XCTAssertEqual(requestCount, 0)
    }

    func testExplicitSSHFixturePreservesPermissionsAndReclaimsStaleStaging() async throws {
        let draft = try Self.explicitSSHFixtureDraft()
        let client = NativeOpenSSHClient()
        let identifier = UUID().uuidString.lowercased()
        let remoteDirectory = "/tmp/chatos-ssh-integration-\(identifier)"
        let staleName = ".chatos-upload-\(UUID().uuidString.lowercased()).tmp"
        let localRoot = FileManager.default.temporaryDirectory.appendingPathComponent(
            "chatos-ssh-integration-\(identifier)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: localRoot, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: localRoot) }
        let upload = localRoot.appendingPathComponent("result.bin")
        let download = localRoot.appendingPathComponent("download.bin")
        let payload = Data("complete-new".utf8)
        try payload.write(to: upload)

        try await client.prepareAuthenticatedConnection(
            draft: draft,
            verificationCode: ""
        )
        do {
            let setup = try await client.runCommand(
                draft: draft,
                command: "umask 077; mkdir -- '\(remoteDirectory)'; "
                    + "printf old > '\(remoteDirectory)/result.bin'; "
                    + "chmod 640 '\(remoteDirectory)/result.bin'; "
                    + "printf stale > '\(remoteDirectory)/\(staleName)'; "
                    + "touch -t 202001010000 '\(remoteDirectory)/\(staleName)'",
                timeoutSeconds: 20,
                maximumOutputCharacters: 4_096
            )
            XCTAssertEqual(setup.exitCode, 0, setup.stderr)

            try await client.uploadFile(
                draft: draft,
                localURL: upload,
                remotePath: remoteDirectory + "/result.bin",
                overwrite: true
            )
            let verification = try await client.runCommand(
                draft: draft,
                command: "cat '\(remoteDirectory)/result.bin'; printf '\\n'; "
                    + "(stat -c '%a' '\(remoteDirectory)/result.bin' 2>/dev/null "
                    + "|| stat -f '%Lp' '\(remoteDirectory)/result.bin'); "
                    + "test ! -e '\(remoteDirectory)/\(staleName)'",
                timeoutSeconds: 20,
                maximumOutputCharacters: 4_096
            )
            XCTAssertEqual(verification.exitCode, 0, verification.stderr)
            XCTAssertEqual(
                verification.stdout.trimmingCharacters(in: .whitespacesAndNewlines),
                "complete-new\n640"
            )

            try await client.downloadFile(
                draft: draft,
                remotePath: remoteDirectory + "/result.bin",
                localURL: download,
                overwrite: false
            )
            XCTAssertEqual(try Data(contentsOf: download), payload)
        } catch {
            _ = try? await client.runCommand(
                draft: draft,
                command: "rm -rf -- '\(remoteDirectory)'",
                timeoutSeconds: 20,
                maximumOutputCharacters: 4_096
            )
            throw error
        }
        let cleanup = try await client.runCommand(
            draft: draft,
            command: "rm -rf -- '\(remoteDirectory)'",
            timeoutSeconds: 20,
            maximumOutputCharacters: 4_096
        )
        XCTAssertEqual(cleanup.exitCode, 0, cleanup.stderr)
    }

    func testExplicitSSHFixtureCancellationAfterCommitRequiresReconciliation() async throws {
        let draft = try Self.explicitSSHFixtureDraft()
        let client = NativeOpenSSHClient(remoteCommitAcknowledgementDelaySeconds: 5)
        let verifier = NativeOpenSSHClient()
        let identifier = UUID().uuidString.lowercased()
        let remoteDirectory = "/tmp/chatos-ssh-uncertain-\(identifier)"
        let remotePath = remoteDirectory + "/result.bin"
        let localRoot = FileManager.default.temporaryDirectory.appendingPathComponent(
            "chatos-ssh-uncertain-\(identifier)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: localRoot, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: localRoot) }
        let upload = localRoot.appendingPathComponent("result.bin")
        try Data("committed-before-ack".utf8).write(to: upload)

        try await client.prepareAuthenticatedConnection(draft: draft, verificationCode: "")
        let setup = try await verifier.runCommand(
            draft: draft,
            command: "mkdir -- '\(remoteDirectory)'",
            timeoutSeconds: 20,
            maximumOutputCharacters: 4_096
        )
        XCTAssertEqual(setup.exitCode, 0, setup.stderr)
        let uploadTask = Task {
            try await client.uploadFile(
                draft: draft,
                localURL: upload,
                remotePath: remotePath,
                overwrite: true
            )
        }
        var observedCommittedTarget = false
        for _ in 0..<100 {
            let observation = try await verifier.runCommand(
                draft: draft,
                command: "test -f '\(remotePath)'",
                timeoutSeconds: 5,
                maximumOutputCharacters: 4_096
            )
            if observation.exitCode == 0 {
                observedCommittedTarget = true
                break
            }
            try await Task.sleep(for: .milliseconds(50))
        }
        XCTAssertTrue(observedCommittedTarget)
        uploadTask.cancel()
        do {
            try await uploadTask.value
            XCTFail("丢失提交确认的上传不能报告普通成功")
        } catch let failure as RemoteFileMutationFailure {
            XCTAssertTrue(
                failure.requiresReconciliation,
                "expected reconciliation-required failure, received \(failure)"
            )
        } catch {
            XCTFail("应返回待对账结果，而不是 \(error)")
        }
        let verification = try await verifier.runCommand(
            draft: draft,
            command: "cat '\(remotePath)'",
            timeoutSeconds: 20,
            maximumOutputCharacters: 4_096
        )
        XCTAssertEqual(verification.exitCode, 0, verification.stderr)
        XCTAssertEqual(verification.stdout, "committed-before-ack")
        _ = try await verifier.runCommand(
            draft: draft,
            command: "rm -rf -- '\(remoteDirectory)'",
            timeoutSeconds: 20,
            maximumOutputCharacters: 4_096
        )
    }

    func testExplicitSSHFixtureDisconnectAfterCommitRequiresReconciliation() async throws {
        let draft = try Self.explicitSSHFixtureDraft()
        let container = try Self.explicitSSHFixtureContainerName()
        let client = NativeOpenSSHClient(remoteCommitAcknowledgementDelaySeconds: 30)
        let verifier = NativeOpenSSHClient()
        let identifier = UUID().uuidString.lowercased()
        let remoteDirectory = "/tmp/chatos-ssh-disconnect-\(identifier)"
        let remotePath = remoteDirectory + "/result.bin"
        let localRoot = FileManager.default.temporaryDirectory.appendingPathComponent(
            "chatos-ssh-disconnect-\(identifier)",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: localRoot, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: localRoot) }
        let upload = localRoot.appendingPathComponent("result.bin")
        try Data("committed-before-disconnect".utf8).write(to: upload)

        try await client.prepareAuthenticatedConnection(draft: draft, verificationCode: "")
        let setup = try await verifier.runCommand(
            draft: draft,
            command: "mkdir -- '\(remoteDirectory)'",
            timeoutSeconds: 20,
            maximumOutputCharacters: 4_096
        )
        XCTAssertEqual(setup.exitCode, 0, setup.stderr)
        let uploadTask = Task {
            try await client.uploadFile(
                draft: draft,
                localURL: upload,
                remotePath: remotePath,
                overwrite: true
            )
        }
        var observedCommittedTarget = false
        for _ in 0..<100 {
            let observation = try await verifier.runCommand(
                draft: draft,
                command: "test -f '\(remotePath)'",
                timeoutSeconds: 5,
                maximumOutputCharacters: 4_096
            )
            if observation.exitCode == 0 {
                observedCommittedTarget = true
                break
            }
            try await Task.sleep(for: .milliseconds(50))
        }
        XCTAssertTrue(observedCommittedTarget)

        var fixtureWasStopped = false
        defer {
            if fixtureWasStopped {
                _ = try? Self.runDockerFixtureCommand(["start", container])
            }
        }
        try Self.runDockerFixtureCommand(["stop", "--time", "0", container])
        fixtureWasStopped = true
        do {
            try await uploadTask.value
            XCTFail("链路在提交确认前断开不能报告普通成功")
        } catch let failure as RemoteFileMutationFailure {
            XCTAssertTrue(
                failure.requiresReconciliation,
                "expected reconciliation-required failure, received \(failure)"
            )
        } catch {
            XCTFail("应返回待对账结果，而不是 \(error)")
        }

        try Self.runDockerFixtureCommand(["start", container])
        fixtureWasStopped = false
        var verification: NativeRemoteCommandResult?
        for _ in 0..<100 {
            if let result = try? await verifier.runCommand(
                draft: draft,
                command: "cat '\(remotePath)'",
                timeoutSeconds: 5,
                maximumOutputCharacters: 4_096
            ), result.exitCode == 0 {
                verification = result
                break
            }
            try await Task.sleep(for: .milliseconds(100))
        }
        let committed = try XCTUnwrap(verification)
        XCTAssertEqual(committed.stdout, "committed-before-disconnect")
        _ = try await verifier.runCommand(
            draft: draft,
            command: "rm -rf -- '\(remoteDirectory)'",
            timeoutSeconds: 20,
            maximumOutputCharacters: 4_096
        )
    }

    func testCoreTerminalPortWrapsNativeSession() async throws {
        let upstream = RemoteConnectionUpstreamStub()
        let tester = RemoteConnectionTesterSpy()
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-remote-port-test-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let service = NativeRemoteConnectionService(
            upstream: upstream,
            tester: tester,
            credentialStore: NativeRemoteConnectionCredentialStore(
                secretStore: NativeConnectorSecretStore(rootURL: root)
            )
        )
        let connection = try await service.createConnection(Self.passwordDraft)
        let provider: any RemoteTerminalSessionProviding = service

        let session = try await provider.openRemoteTerminalSession(
            connectionID: connection.id,
            verificationCode: "123456"
        )

        XCTAssertFalse(session.isRunning)
        session.close()
        let requestCount = await tester.requestCount()
        XCTAssertEqual(requestCount, 0)
    }

    private static let passwordDraft = RemoteConnectionDraft(
        name: "Server",
        host: "server.example.com",
        port: 22,
        username: "root",
        authenticationType: .password,
        password: "local-secret",
        privateKeyPath: nil,
        certificatePath: nil,
        defaultRemotePath: "/srv/app",
        hostKeyPolicy: .acceptNew,
        localConnectorDeviceID: "legacy-device",
        localConnectorWorkspaceID: "legacy-workspace",
        jumpEnabled: false,
        jumpConnectionID: nil,
        jumpHost: nil,
        jumpPort: nil,
        jumpUsername: nil,
        jumpPrivateKeyPath: nil,
        jumpCertificatePath: nil,
        jumpPassword: nil
    )

    private static func explicitSSHFixtureDraft() throws -> RemoteConnectionDraft {
        let environment = ProcessInfo.processInfo.environment
        guard let host = environment["CHATOS_SSH_TEST_HOST"],
              let portText = environment["CHATOS_SSH_TEST_PORT"],
              let port = Int(portText),
              let username = environment["CHATOS_SSH_TEST_USER"] else {
            throw XCTSkip(
                "Set CHATOS_SSH_TEST_HOST/PORT/USER and a password or key for an isolated SSH fixture"
            )
        }
        var draft = passwordDraft
        draft.host = host
        draft.port = port
        draft.username = username
        if let privateKey = environment["CHATOS_SSH_TEST_PRIVATE_KEY"],
           !privateKey.isEmpty {
            draft.authenticationType = .privateKey
            draft.password = nil
            draft.privateKeyPath = privateKey
        } else if let password = environment["CHATOS_SSH_TEST_PASSWORD"] {
            draft.password = password
        } else {
            throw XCTSkip("Set CHATOS_SSH_TEST_PASSWORD or CHATOS_SSH_TEST_PRIVATE_KEY")
        }
        draft.defaultRemotePath = "/tmp"
        return draft
    }

    private static func explicitSSHFixtureContainerName() throws -> String {
        guard let name = ProcessInfo.processInfo.environment[
            "CHATOS_SSH_TEST_DISCONNECT_CONTAINER"
        ], !name.isEmpty else {
            throw XCTSkip(
                "Set CHATOS_SSH_TEST_DISCONNECT_CONTAINER to an isolated Docker SSH fixture"
            )
        }
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "_.-"))
        guard name.unicodeScalars.allSatisfy(allowed.contains) else {
            throw XCTSkip("SSH fixture container name contains unsupported characters")
        }
        return name
    }

    private static func processIsAlive(_ processID: Int32) -> Bool {
        errno = 0
        return Darwin.kill(processID, 0) == 0 || errno == EPERM
    }

    @discardableResult
    private static func runDockerFixtureCommand(_ arguments: [String]) throws -> Int32 {
        let process = Process()
        let diagnostics = Pipe()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = ["docker"] + arguments
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = diagnostics
        process.standardError = diagnostics
        try process.run()
        process.waitUntilExit()
        let output = diagnostics.fileHandleForReading.readDataToEndOfFile()
        guard process.terminationStatus == 0 else {
            throw NSError(
                domain: "ChatOSSSHFixture",
                code: Int(process.terminationStatus),
                userInfo: [
                    NSLocalizedDescriptionKey: String(decoding: output, as: UTF8.self),
                ]
            )
        }
        return process.terminationStatus
    }
}

private actor RemoteConnectionUpstreamStub: RemoteConnectionServicing {
    private var connections: [RemoteConnection] = []
    private var createdDraft: RemoteConnectionDraft?
    private var getRequests = 0
    private var listRequests = 0
    private var updateRequests = 0

    func listConnections() async throws -> [RemoteConnection] {
        listRequests += 1
        return connections
    }

    func getConnection(id: String) async throws -> RemoteConnection? {
        getRequests += 1
        return connections.first { $0.id == id }
    }

    func createConnection(_ draft: RemoteConnectionDraft) async throws -> RemoteConnection {
        createdDraft = draft
        let connection = RemoteConnection(
            id: "connection-1",
            name: draft.name ?? "Server",
            host: draft.host,
            port: draft.port,
            username: draft.username,
            authenticationType: draft.authenticationType,
            hasPassword: false,
            hasPrivateKeyPath: false,
            hasCertificatePath: false,
            defaultRemotePath: draft.defaultRemotePath,
            hostKeyPolicy: draft.hostKeyPolicy,
            localConnectorDeviceID: draft.localConnectorDeviceID,
            localConnectorWorkspaceID: draft.localConnectorWorkspaceID,
            jumpEnabled: draft.jumpEnabled,
            jumpConnectionID: draft.jumpConnectionID,
            jumpHost: draft.jumpHost,
            jumpPort: draft.jumpPort,
            jumpUsername: draft.jumpUsername,
            hasJumpPrivateKeyPath: false,
            hasJumpCertificatePath: false,
            hasJumpPassword: false,
            lastActiveAt: nil
        )
        connections = [connection]
        return connection
    }

    func updateConnection(
        id: String,
        draft: RemoteConnectionDraft
    ) async throws -> RemoteConnection {
        updateRequests += 1
        let updated = RemoteConnection(
            id: id,
            name: draft.name ?? "Server",
            host: draft.host,
            port: draft.port,
            username: draft.username,
            authenticationType: draft.authenticationType,
            hasPassword: false,
            hasPrivateKeyPath: false,
            hasCertificatePath: false,
            defaultRemotePath: draft.defaultRemotePath,
            hostKeyPolicy: draft.hostKeyPolicy,
            localConnectorDeviceID: draft.localConnectorDeviceID,
            localConnectorWorkspaceID: draft.localConnectorWorkspaceID,
            jumpEnabled: draft.jumpEnabled,
            jumpConnectionID: draft.jumpConnectionID,
            jumpHost: draft.jumpHost,
            jumpPort: draft.jumpPort,
            jumpUsername: draft.jumpUsername,
            hasJumpPrivateKeyPath: false,
            hasJumpCertificatePath: false,
            hasJumpPassword: false,
            lastActiveAt: nil
        )
        connections.removeAll { $0.id == id }
        connections.append(updated)
        return updated
    }

    func deleteConnection(id: String) async throws {
        connections.removeAll { $0.id == id }
    }

    func testDraft(
        _ draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        XCTFail("Native service must not use the cloud test endpoint")
        return .init(success: false, message: nil)
    }

    func testSaved(
        id: String,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        XCTFail("Native service must not use the cloud test endpoint")
        return .init(success: false, message: nil)
    }

    func lastCreatedDraft() -> RemoteConnectionDraft? {
        createdDraft
    }

    func getRequestCount() -> Int { getRequests }

    func listRequestCount() -> Int { listRequests }

    func updateRequestCount() -> Int { updateRequests }
}

private actor RemoteConnectionTesterSpy: NativeRemoteConnectionTesting {
    private var draft: RemoteConnectionDraft?
    private var requests = 0

    func test(
        draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        requests += 1
        self.draft = draft
        return .init(success: true, message: "ok")
    }

    func lastDraft() -> RemoteConnectionDraft? {
        draft
    }

    func requestCount() -> Int {
        requests
    }
}
