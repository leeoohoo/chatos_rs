import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
import ChatOSCore
import ChatOSMediaWorkerClient
import ChatOSWorkerRuntime
import CryptoKit
import Darwin
import Foundation
import XCTest
@testable import ChatOSConnector

final class CodeReadWorkerProcessIsolationTests: XCTestCase {
    func testSharedMachineQueueDoesNotSpawnWorkerBeforeAdmission() async throws {
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        let marker = fixture.root.appendingPathComponent("worker-started")
        let script = fixture.root.appendingPathComponent("marker-worker.sh")
        try Data("#!/bin/sh\n/usr/bin/touch '\(marker.path)'\nexec /bin/sleep 60\n".utf8)
            .write(to: script)
        try FileManager.default.setAttributes(
            [.posixPermissions: NSNumber(value: Int16(0o700))],
            ofItemAtPath: script.path
        )
        let machineGovernor = ClientMachineResourceGovernor(budget: .init(
            availableResourceUnits: 2,
            reservedHostResourceUnits: 2,
            maximumAgentRuns: 1,
            maximumMediaWorkers: 2,
            maximumCodeReadWorkers: 2
        ))
        let agent = try await machineGovernor.acquire(context: .init(
            kind: .agentRun,
            priority: .interactive,
            operation: "occupied-agent"
        ))
        let fileGovernor = ClientFileWorkerResourceGovernor(
            budget: .init(
                totalConcurrentWorkers: 3,
                maximumMediaWorkers: 2,
                maximumCodeReadWorkers: 2
            ),
            machineGovernor: machineGovernor
        )
        let request = Task {
            try await CodeReadWorkerProcessClient(
                executableURL: script,
                timeout: 30,
                resourceGovernor: fileGovernor
            ).execute(
                operation: "read_file_raw",
                arguments: ["path": .string("README.md")],
                projectRoot: fixture.root
            )
        }
        defer { request.cancel() }
        for _ in 0..<200 {
            let snapshot = await machineGovernor.snapshot()
            if snapshot.queued.contains(where: { $0.kind == .codeReadWorker }) { break }
            try await Task.sleep(for: .milliseconds(5))
        }

        var snapshot = await machineGovernor.snapshot()
        XCTAssertEqual(snapshot.queued.filter { $0.kind == .codeReadWorker }.count, 1)
        try await Task.sleep(for: .milliseconds(50))
        XCTAssertFalse(FileManager.default.fileExists(atPath: marker.path))

        await agent.releaseAndWait()
        for _ in 0..<200 {
            if FileManager.default.fileExists(atPath: marker.path) { break }
            try await Task.sleep(for: .milliseconds(5))
        }
        XCTAssertTrue(FileManager.default.fileExists(atPath: marker.path))
        request.cancel()
        _ = try? await request.value
        snapshot = await machineGovernor.snapshot()
        XCTAssertTrue(snapshot.active.isEmpty)
    }

    func testMediaBurstCannotConsumeReservedCodeReadWorkerSlot() async throws {
        let executable = try realWorkerExecutable()
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        let governor = ClientFileWorkerResourceGovernor(budget: .init(
            totalConcurrentWorkers: 3,
            maximumMediaWorkers: 2,
            maximumCodeReadWorkers: 2
        ))
        let script = fixture.root.appendingPathComponent("hanging-media-worker.sh")
        try Data("#!/bin/sh\nexec /bin/sleep 60\n".utf8).write(to: script)
        try FileManager.default.setAttributes(
            [.posixPermissions: NSNumber(value: Int16(0o700))],
            ofItemAtPath: script.path
        )
        let source = fixture.root.appendingPathComponent("source.png")
        let firstDestination = fixture.root.appendingPathComponent("first.png")
        let secondDestination = fixture.root.appendingPathComponent("second.png")
        try Data("image".utf8).write(to: source)
        XCTAssertTrue(FileManager.default.createFile(
            atPath: firstDestination.path,
            contents: nil
        ))
        XCTAssertTrue(FileManager.default.createFile(
            atPath: secondDestination.path,
            contents: nil
        ))
        let client = MediaWorkerProcessClient(
            executableURL: script,
            timeout: 30,
            resourceGovernor: governor
        )
        let firstMedia = Task {
            try await client.normalizePNG(
                sourceURL: source,
                destinationURL: firstDestination,
                ownerUserID: "resource-test",
                maximumSourceBytes: 1_024,
                maximumSourcePixelCount: 1_024,
                maximumOutputPixelSize: 32,
                maximumOutputBytes: 1_024
            )
        }
        let secondMedia = Task {
            try await client.normalizePNG(
                sourceURL: source,
                destinationURL: secondDestination,
                ownerUserID: "resource-test",
                maximumSourceBytes: 1_024,
                maximumSourcePixelCount: 1_024,
                maximumOutputPixelSize: 32,
                maximumOutputBytes: 1_024
            )
        }
        defer {
            firstMedia.cancel()
            secondMedia.cancel()
        }
        for _ in 0..<1_000 {
            if await governor.snapshot().active.count == 2 { break }
            try await Task.sleep(for: .milliseconds(5))
        }
        let saturatedSnapshot = await governor.snapshot()
        XCTAssertEqual(saturatedSnapshot.active.count, 2)
        let readable = fixture.root.appendingPathComponent("README.md")
        try Data("reserved-code-slot\n".utf8).write(to: readable)

        let result = try await CodeReadWorkerProcessClient(
            executableURL: executable,
            timeout: 5,
            resourceGovernor: governor
        ).execute(
            operation: "read_file_raw",
            arguments: ["path": .string("README.md")],
            projectRoot: fixture.root,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepURL: nil
        )

        guard case let .object(payload) = result,
              case let .string(content)? = payload["content"] else {
            return XCTFail("real worker returned an invalid read payload")
        }
        XCTAssertEqual(content, "reserved-code-slot\n")
        let snapshot = await governor.snapshot()
        XCTAssertEqual(snapshot.metrics.maximumObservedActive, 3)
        firstMedia.cancel()
        secondMedia.cancel()
        _ = try? await firstMedia.value
        _ = try? await secondMedia.value
    }

    func testRealWorkerReadsThroughAuthenticatedProcessBoundary() async throws {
        let packageRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        #if DEBUG
        let configurations = ["Debug", "Release"]
        #else
        let configurations = ["Release", "Debug"]
        #endif
        let candidates = configurations.map {
            packageRoot.appendingPathComponent(".build/out/Products/\($0)/ChatOSCodeReadWorker")
        }
        guard let executable = candidates.first(where: {
            FileManager.default.isExecutableFile(atPath: $0.path)
        }) else {
            throw XCTSkip("Build ChatOSCodeReadWorker before running the process integration test")
        }
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        try Data("isolated-content\n".utf8)
            .write(to: fixture.root.appendingPathComponent("README.md"))
        try Data(#"{"scripts":{"dev":"vite"}}"#.utf8)
            .write(to: fixture.root.appendingPathComponent("package.json"))

        let result = try await CodeReadWorkerProcessClient(
            executableURL: executable,
            timeout: 5
        ).execute(
            operation: "read_file_raw",
            arguments: ["path": .string("README.md")],
            projectRoot: fixture.root,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepURL: nil
        )

        guard case let .object(payload) = result,
              case let .string(content)? = payload["content"] else {
            return XCTFail("real worker returned an invalid read payload")
        }
        XCTAssertEqual(content, "isolated-content\n")

        let presented = try await CodeReadWorkerProcessClient(
            executableURL: executable,
            timeout: 5
        ).execute(
            operation: "open_file_in_pet",
            arguments: [
                "path": .string("README.md"),
                "line": .number(1),
                "mode": .string("preview"),
            ],
            projectRoot: fixture.root,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepURL: nil
        )
        guard case let .object(presentationPayload) = presented,
              case let .string(relativePath)? = presentationPayload["relative_path"] else {
            return XCTFail("real worker returned an invalid presentation payload")
        }
        XCTAssertEqual(relativePath, "README.md")

        let listing = try await CodeReadWorkerProcessClient(
            executableURL: executable,
            timeout: 5
        ).execute(
            operation: "project_list_v1",
            arguments: ["path": .string(".")],
            projectRoot: fixture.root,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepURL: nil
        )
        guard case let .object(listingPayload) = listing,
              case let .array(entries)? = listingPayload["entries"] else {
            return XCTFail("real worker returned an invalid project listing")
        }
        XCTAssertTrue(entries.contains { entry in
            guard case let .object(fields) = entry,
                  case let .string(name)? = fields["name"] else { return false }
            return name == "README.md"
        })

        let analysis = try await CodeReadWorkerProcessClient(
            executableURL: executable,
            timeout: 5
        ).execute(
            operation: "project_run_analyze_v1",
            arguments: [:],
            projectRoot: fixture.root,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepURL: nil
        )
        guard case let .object(analysisPayload) = analysis,
              case let .array(targets)? = analysisPayload["targets"] else {
            return XCTFail("real worker returned an invalid project analysis payload")
        }
        XCTAssertTrue(targets.contains { target in
            guard case let .object(fields) = target,
                  case let .string(command)? = fields["command"] else { return false }
            return command == "npm run dev"
        })

        _ = try await CodeReadWorkerProcessClient(
            executableURL: executable,
            timeout: 5
        ).execute(
            operation: "project_create_directory_v1",
            arguments: [
                "path": .string("Generated"),
                "create_only": .bool(true),
            ],
            projectRoot: fixture.root,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepURL: nil
        )
        do {
            _ = try await CodeReadWorkerProcessClient(
                executableURL: executable,
                timeout: 5
            ).execute(
                operation: "project_create_directory_v1",
                arguments: [
                    "path": .string("Generated"),
                    "create_only": .bool(true),
                ],
                projectRoot: fixture.root,
                requestCWD: nil,
                defaultToolRoot: nil,
                ripgrepURL: nil
            )
            XCTFail("real worker accepted an existing create-only directory")
        } catch let error as CodeReadWorkerProcessClientError {
            guard case let .requestRejected(code, _) = error else { throw error }
            XCTAssertEqual(code, "already_exists")
        }
        _ = try await CodeReadWorkerProcessClient(
            executableURL: executable,
            timeout: 5
        ).execute(
            operation: "project_write_file_v1",
            arguments: [
                "path": .string("Generated/result.txt"),
                "content": .string("worker mutation\n"),
                "create_only": .bool(true),
            ],
            projectRoot: fixture.root,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepURL: nil
        )
        XCTAssertEqual(
            try String(contentsOf: fixture.root.appendingPathComponent("Generated/result.txt")),
            "worker mutation\n"
        )
        _ = try await CodeReadWorkerProcessClient(
            executableURL: executable,
            timeout: 5
        ).execute(
            operation: "project_delete_entry_v1",
            arguments: ["path": .string("Generated"), "recursive": .bool(true)],
            projectRoot: fixture.root,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepURL: nil
        )
        XCTAssertFalse(FileManager.default.fileExists(
            atPath: fixture.root.appendingPathComponent("Generated").path
        ))
    }

    func testHungProjectAnalysisDoesNotBlockIndependentWorker() async throws {
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        let hanging = try fixture.makeWorker(name: "hanging-worker.py", hangs: true)
        let responsive = try fixture.makeWorker(name: "responsive-worker.py", hangs: false)
        let blockedCompletion = CompletionFlag()
        let blocked = Task {
            do {
                let result = try await CodeReadWorkerProcessClient(
                    executableURL: hanging,
                    timeout: 20
                ).execute(
                    operation: "project_run_analyze_v1",
                    arguments: [:],
                    projectRoot: fixture.root,
                    requestCWD: nil,
                    defaultToolRoot: nil,
                    ripgrepURL: nil
                )
                await blockedCompletion.markCompleted()
                return result
            } catch {
                await blockedCompletion.markCompleted()
                throw error
            }
        }
        try await Task.sleep(for: .milliseconds(100))

        let result = try await CodeReadWorkerProcessClient(
            executableURL: responsive,
            timeout: 10
        ).execute(
            operation: "project_run_analyze_v1",
            arguments: [:],
            projectRoot: fixture.root,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepURL: nil
        )
        let hangingWorkerAlreadyCompleted = await blockedCompletion.isCompleted
        XCTAssertFalse(hangingWorkerAlreadyCompleted)
        guard case let .object(payload) = result,
              case let .string(value)? = payload["worker"] else {
            return XCTFail("responsive worker returned an invalid result")
        }
        XCTAssertEqual(value, "independent")

        blocked.cancel()
        do {
            _ = try await blocked.value
            XCTFail("cancelled hanging worker should not return")
        } catch is CancellationError {}
    }

    func testHungProjectMutationDoesNotBlockIndependentWorker() async throws {
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        let hanging = try fixture.makeWorker(name: "hanging-mutation-worker.py", hangs: true)
        let responsive = try fixture.makeWorker(name: "responsive-mutation-worker.py", hangs: false)
        let blockedCompletion = CompletionFlag()
        let blocked = Task {
            do {
                let result = try await CodeReadWorkerProcessClient(
                    executableURL: hanging,
                    timeout: 20
                ).execute(
                    operation: "project_write_file_v1",
                    arguments: ["path": .string("blocked.txt"), "content": .string("blocked")],
                    projectRoot: fixture.root,
                    requestCWD: nil,
                    defaultToolRoot: nil,
                    ripgrepURL: nil
                )
                await blockedCompletion.markCompleted()
                return result
            } catch {
                await blockedCompletion.markCompleted()
                throw error
            }
        }
        try await Task.sleep(for: .milliseconds(100))

        let result = try await CodeReadWorkerProcessClient(
            executableURL: responsive,
            timeout: 10
        ).execute(
            operation: "project_write_file_v1",
            arguments: ["path": .string("independent.txt"), "content": .string("independent")],
            projectRoot: fixture.root,
            requestCWD: nil,
            defaultToolRoot: nil,
            ripgrepURL: nil
        )
        let hangingWorkerAlreadyCompleted = await blockedCompletion.isCompleted
        XCTAssertFalse(hangingWorkerAlreadyCompleted)
        guard case let .object(payload) = result,
              case let .string(value)? = payload["worker"] else {
            return XCTFail("responsive worker returned an invalid result")
        }
        XCTAssertEqual(value, "independent")

        blocked.cancel()
        do {
            _ = try await blocked.value
            XCTFail("cancelled hanging mutation should not return")
        } catch is CancellationError {}
    }

    func testRealWorkerResolvesVerifiesAndMaterializesAttachment() async throws {
        let executable = try realWorkerExecutable()
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        let workspace = try WorkerFixture()
        defer { workspace.dispose() }
        let attachment = try fixture.makeAttachment(Data("process attachment\n".utf8))
        let client = CodeReadWorkerProcessClient(executableURL: executable, timeout: 5)

        var resolveArguments = attachment.arguments
        resolveArguments["offset"] = .number(8)
        resolveArguments["limit"] = .number(10)
        let resolved = try await client.execute(
            operation: "attachment_resolve_v1",
            arguments: resolveArguments,
            projectRoot: fixture.root
        )
        guard case let .object(resolvePayload) = resolved,
              case let .string(content)? = resolvePayload["content"] else {
            return XCTFail("real worker returned an invalid attachment payload")
        }
        XCTAssertEqual(content, "attachment")

        let verified = try await client.execute(
            operation: "attachment_verify_file_v1",
            arguments: attachment.arguments,
            projectRoot: fixture.root
        )
        guard case let .object(verifyPayload) = verified,
              case let .string(relativePath)? = verifyPayload["relative_path"] else {
            return XCTFail("real worker returned an invalid verification payload")
        }
        XCTAssertEqual(relativePath, attachment.relativePath)

        var materializeArguments = attachment.arguments
        materializeArguments.merge(try workspace.identityArguments()) { _, new in new }
        let materialized = try await client.execute(
            operation: "attachment_materialize_v1",
            arguments: materializeArguments,
            projectRoot: fixture.root
        )
        guard case let .object(materializePayload) = materialized,
              case let .string(workspacePath)? = materializePayload["relative_path"] else {
            return XCTFail("real worker returned an invalid materialize payload")
        }
        XCTAssertEqual(
            try Data(contentsOf: workspace.root.appendingPathComponent(workspacePath)),
            attachment.data
        )
    }

    func testRealWorkerStreamsAttachmentAuthorizationWithoutJSONPayloadCopies() async throws {
        let executable = try realWorkerExecutable()
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        let first = Data("streamed attachment\n".utf8)
        let second = Data(repeating: 0xa5, count: 256 * 1_024)
        let tokens = [UUID().uuidString.lowercased(), UUID().uuidString.lowercased()]
        let client = CodeReadWorkerProcessClient(executableURL: executable, timeout: 5)

        let result = try await client.execute(
            operation: "attachment_authorize_v1",
            arguments: [
                "owner_user_id": .string("stream-owner"),
                "conversation_id": .string("stream-conversation"),
                "items": .array(tokens.indices.map { index in
                    .object([
                        "token": .string(tokens[index]),
                        "byte_size": .number(Double([first, second][index].count)),
                    ])
                }),
            ],
            binaryPayloads: [first, second],
            projectRoot: fixture.root
        )
        guard case let .object(payload) = result,
              case let .array(items)? = payload["items"],
              items.count == 2 else {
            return XCTFail("real worker returned an invalid authorization payload")
        }
        let directory = fixture.root
            .appendingPathComponent(processSHA256(Data("stream-owner".utf8)))
            .appendingPathComponent(processSHA256(Data("stream-conversation".utf8)))
        XCTAssertEqual(try Data(contentsOf: directory.appendingPathComponent(tokens[0])), first)
        XCTAssertEqual(try Data(contentsOf: directory.appendingPathComponent(tokens[1])), second)
        XCTAssertEqual(
            try FileManager.default.contentsOfDirectory(atPath: directory.path).sorted(),
            tokens.sorted()
        )
        for (index, rawItem) in items.enumerated() {
            guard case let .object(item) = rawItem,
                  case let .string(digest)? = item["sha256"] else {
                return XCTFail("real worker returned an invalid digest")
            }
            XCTAssertEqual(digest, processSHA256([first, second][index]))
        }
    }

    func testRealWorkerVerifiesPreviewFileIdentity() async throws {
        let executable = try realWorkerExecutable()
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        let data = Data(repeating: 0x39, count: 128 * 1_024)
        let file = fixture.root.appendingPathComponent("preview.bin")
        try data.write(to: file)
        let result = try await CodeReadWorkerProcessClient(
            executableURL: executable,
            timeout: 5
        ).execute(
            operation: "verify_file_identity_v1",
            arguments: [
                "name": .string(file.lastPathComponent),
                "byte_size": .number(Double(data.count)),
                "sha256": .string(processSHA256(data)),
            ],
            projectRoot: fixture.root
        )
        guard case let .object(fields) = result,
              case let .number(byteCount)? = fields["byte_count"] else {
            return XCTFail("real worker returned an invalid file identity")
        }
        XCTAssertEqual(byteCount, Double(data.count))
    }

    func testRealWorkersStageAndRestoreArtifactsIndependently() async throws {
        let executable = try realWorkerExecutable()
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        let client = CodeReadWorkerProcessClient(executableURL: executable, timeout: 10)
        let firstData = Data(repeating: 0x41, count: 512 * 1_024)
        let secondData = Data(repeating: 0x42, count: 640 * 1_024)
        let firstSource = fixture.root.appendingPathComponent("first-source.bin")
        let secondSource = fixture.root.appendingPathComponent("second-source.bin")
        try firstData.write(to: firstSource)
        try secondData.write(to: secondSource)
        let staging = fixture.root.appendingPathComponent("staging", isDirectory: true)

        async let first = AgentArtifactUploadStager.stage(
            from: firstSource,
            in: staging,
            expectedBytes: firstData.count,
            expectedSHA256: processSHA256(firstData),
            codeReadWorker: client
        )
        async let second = AgentArtifactUploadStager.stage(
            from: secondSource,
            in: staging,
            expectedBytes: secondData.count,
            expectedSHA256: processSHA256(secondData),
            codeReadWorker: client
        )
        let (firstStaged, secondStaged) = try await (first, second)
        defer {
            AgentArtifactUploadStager.discard(firstStaged)
            AgentArtifactUploadStager.discard(secondStaged)
        }
        XCTAssertNotEqual(firstStaged.token, secondStaged.token)
        XCTAssertEqual(try Data(contentsOf: firstStaged.fileURL), firstData)
        XCTAssertEqual(try Data(contentsOf: secondStaged.fileURL), secondData)

        let restored = fixture.root
            .appendingPathComponent("restored", isDirectory: true)
            .appendingPathComponent("first.bin")
        try await AgentArtifactFileRestorer.restore(
            .init(
                fileURL: firstStaged.fileURL,
                size: firstData.count,
                sha256: processSHA256(firstData)
            ),
            to: restored,
            expectedBytes: firstData.count,
            expectedSHA256: processSHA256(firstData),
            codeReadWorker: client
        )
        XCTAssertEqual(try Data(contentsOf: restored), firstData)
        let attributes = try FileManager.default.attributesOfItem(atPath: restored.path)
        XCTAssertEqual((attributes[.posixPermissions] as? NSNumber)?.intValue, 0o600)
    }

    func testRealWorkerCleanupPreservesLockedAndFreshTemporaryFiles() async throws {
        let executable = try realWorkerExecutable()
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        let staging = fixture.root
            .appendingPathComponent("LocalAgent/local-agent.artifact-staging", isDirectory: true)
        try FileManager.default.createDirectory(at: staging, withIntermediateDirectories: true)
        let stale = staging.appendingPathComponent(
            ".chatos-artifact-copy-\(UUID().uuidString.lowercased()).tmp"
        )
        let locked = staging.appendingPathComponent(
            ".chatos-artifact-copy-\(UUID().uuidString.lowercased()).tmp"
        )
        let fresh = staging.appendingPathComponent(
            ".chatos-artifact-copy-\(UUID().uuidString.lowercased()).tmp"
        )
        try Data("stale".utf8).write(to: stale)
        try Data("locked".utf8).write(to: locked)
        try Data("fresh".utf8).write(to: fresh)
        let oldDate = Date(timeIntervalSinceNow: -(25 * 60 * 60))
        try FileManager.default.setAttributes([.modificationDate: oldDate], ofItemAtPath: stale.path)
        try FileManager.default.setAttributes([.modificationDate: oldDate], ofItemAtPath: locked.path)
        let descriptor = locked.path.withCString {
            Darwin.open($0, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
        }
        XCTAssertGreaterThanOrEqual(descriptor, 0)
        defer { if descriptor >= 0 { _ = Darwin.close(descriptor) } }
        XCTAssertEqual(flock(descriptor, LOCK_EX | LOCK_NB), 0)

        let report = try await NativeLocalTemporaryFileMaintenanceService(
            rootURL: fixture.root,
            codeReadWorker: CodeReadWorkerProcessClient(executableURL: executable, timeout: 10)
        ).cleanupStaleFiles()

        XCTAssertEqual(report.deletedEntries, 1)
        XCTAssertEqual(report.lockedEntries, 1)
        XCTAssertFalse(FileManager.default.fileExists(atPath: stale.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: locked.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: fresh.path))
    }

    func testAttachmentVaultProductionAuthorizationUsesBinaryWorker() async throws {
        let executable = try realWorkerExecutable()
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        let client = CodeReadWorkerProcessClient(executableURL: executable, timeout: 5)
        let vault = NativeLocalAgentAttachmentVault(
            rootURL: fixture.root,
            codeReadWorker: client,
            binaryCodeReadWorker: client
        )
        let bytes = Data(repeating: 0x6d, count: 512 * 1_024)
        let specs = try await vault.authorizeInWorker(
            [.init(
                id: "draft-1",
                name: "payload.bin",
                mimeType: "application/octet-stream",
                kind: .file,
                origin: .file,
                data: bytes
            )],
            ownerUserID: "vault-owner",
            conversationID: "vault-conversation"
        )
        let spec = try XCTUnwrap(specs.first)
        XCTAssertEqual(spec.attachmentID, "draft-1")
        XCTAssertEqual(spec.byteSize, UInt64(bytes.count))
        XCTAssertEqual(spec.sha256, processSHA256(bytes))
        let token = String(spec.authorizedLocalRef.dropFirst(
            NativeLocalAgentAttachmentVault.referencePrefix.count
        ))
        let file = fixture.root
            .appendingPathComponent(processSHA256(Data("vault-owner".utf8)))
            .appendingPathComponent(processSHA256(Data("vault-conversation".utf8)))
            .appendingPathComponent(token)
        XCTAssertEqual(try Data(contentsOf: file), bytes)
    }

    func testWorkerExitDuringBinaryUploadDoesNotSignalParentProcess() async throws {
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        let worker = try fixture.makePayloadExitWorker()
        let token = UUID().uuidString.lowercased()
        do {
            _ = try await CodeReadWorkerProcessClient(
                executableURL: worker,
                timeout: 5
            ).execute(
                operation: "attachment_authorize_v1",
                arguments: [
                    "owner_user_id": .string("owner"),
                    "conversation_id": .string("conversation"),
                    "items": .array([.object([
                        "token": .string(token),
                        "byte_size": .number(2 * 1_024 * 1_024),
                    ])]),
                ],
                binaryPayloads: [Data(repeating: 0xcd, count: 2 * 1_024 * 1_024)],
                projectRoot: fixture.root
            )
            XCTFail("worker exit during binary upload must fail")
        } catch let error as CodeReadWorkerProcessClientError {
            XCTAssertEqual(error, .invalidResponse)
        }
    }

    func testHungAttachmentDoesNotBlockIndependentAttachmentWorker() async throws {
        let fixture = try WorkerFixture()
        defer { fixture.dispose() }
        let hanging = try fixture.makeWorker(name: "hanging-attachment-worker.py", hangs: true)
        let responsive = try fixture.makeWorker(
            name: "responsive-attachment-worker.py",
            hangs: false
        )
        let arguments: [String: CodeReadWorkerJSONValue] = [
            "owner_user_id": .string("owner"),
            "conversation_id": .string("conversation"),
            "token": .string(UUID().uuidString.lowercased()),
            "display_name": .string("attachment.txt"),
            "media_type": .string("text/plain"),
            "byte_size": .number(1),
            "sha256": .string(String(repeating: "0", count: 64)),
            "offset": .number(0),
            "limit": .number(1),
        ]
        let blockedCompletion = CompletionFlag()
        let blocked = Task {
            do {
                let result = try await CodeReadWorkerProcessClient(
                    executableURL: hanging,
                    timeout: 20
                ).execute(
                    operation: "attachment_resolve_v1",
                    arguments: arguments,
                    projectRoot: fixture.root
                )
                await blockedCompletion.markCompleted()
                return result
            } catch {
                await blockedCompletion.markCompleted()
                throw error
            }
        }
        try await Task.sleep(for: .milliseconds(100))

        let result = try await CodeReadWorkerProcessClient(
            executableURL: responsive,
            timeout: 10
        ).execute(
            operation: "attachment_resolve_v1",
            arguments: arguments,
            projectRoot: fixture.root
        )
        let hangingWorkerAlreadyCompleted = await blockedCompletion.isCompleted
        XCTAssertFalse(hangingWorkerAlreadyCompleted)
        guard case let .object(payload) = result,
              case let .string(value)? = payload["worker"] else {
            return XCTFail("responsive attachment worker returned an invalid result")
        }
        XCTAssertEqual(value, "independent")

        blocked.cancel()
        do {
            _ = try await blocked.value
            XCTFail("cancelled hanging attachment worker should not return")
        } catch is CancellationError {}
    }
}

private func realWorkerExecutable() throws -> URL {
    let packageRoot = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent()
        .deletingLastPathComponent()
        .deletingLastPathComponent()
    #if DEBUG
    let configurations = ["Debug", "Release"]
    #else
    let configurations = ["Release", "Debug"]
    #endif
    let candidates = configurations.map {
        packageRoot.appendingPathComponent(".build/out/Products/\($0)/ChatOSCodeReadWorker")
    }
    guard let executable = candidates.first(where: {
        FileManager.default.isExecutableFile(atPath: $0.path)
    }) else {
        throw XCTSkip("Build ChatOSCodeReadWorker before running the process integration test")
    }
    return executable
}

private actor CompletionFlag {
    private(set) var isCompleted = false
    func markCompleted() { isCompleted = true }
}

private struct WorkerFixture {
    let root: URL

    init() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("CodeReadIsolation-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    }

    func makeWorker(name: String, hangs: Bool) throws -> URL {
        let url = root.appendingPathComponent(name)
        let behavior = hangs
            ? "kill -STOP $$"
            : "request_id=$(print -r -- \"$request\" | /usr/bin/sed -E 's/.*\"request_id\":\"([^\"]+)\".*/\\1/'); print -r -- \"{\\\"type\\\":\\\"response\\\",\\\"request_id\\\":\\\"$request_id\\\",\\\"ok\\\":true,\\\"result\\\":{\\\"worker\\\":\\\"independent\\\"},\\\"error\\\":null,\\\"retryable\\\":false}\""
        let script = """
        #!/bin/zsh
        IFS= read -r hello
        print -r -- '{"type":"hello_response","ok":true,"selected_protocol_version":3,"worker_build_version":"code-read-worker-v3","helper_instance_id":"fixture-helper","operations":["read_file_raw","read_file_range","list_dir","search_text","read_file","search_files","open_file_in_pet","project_list_v1","project_search_entries_v1","project_run_analyze_v1","project_write_file_v1","project_create_directory_v1","project_delete_entry_v1","attachment_authorize_v1","attachment_resolve_v1","attachment_verify_file_v1","attachment_materialize_v1"],"maximum_inline_bytes":16777216,"maximum_binary_payload_bytes":20971520,"error":null}'
        IFS= read -r request
        \(behavior)
        """
        try Data(script.utf8).write(to: url, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: NSNumber(value: Int16(0o700))],
            ofItemAtPath: url.path
        )
        return url
    }

    func makePayloadExitWorker() throws -> URL {
        let url = root.appendingPathComponent("payload-exit-worker.py")
        let script = """
        #!/bin/zsh
        IFS= read -r hello
        print -r -- '{"type":"hello_response","ok":true,"selected_protocol_version":3,"worker_build_version":"code-read-worker-v3","helper_instance_id":"fixture-helper","operations":["attachment_authorize_v1"],"maximum_inline_bytes":16777216,"maximum_binary_payload_bytes":20971520,"error":null}'
        IFS= read -r request
        exit 0
        """
        try Data(script.utf8).write(to: url, options: .atomic)
        try FileManager.default.setAttributes(
            [.posixPermissions: NSNumber(value: Int16(0o700))],
            ofItemAtPath: url.path
        )
        return url
    }

    func identityArguments() throws -> [String: CodeReadWorkerJSONValue] {
        var status = stat()
        guard root.path.withCString({ Darwin.lstat($0, &status) }) == 0 else {
            throw ProcessFixtureError.invalid
        }
        return [
            "destination_root_path": .string(root.path),
            "destination_root_device": .string(String(status.st_dev)),
            "destination_root_inode": .string(String(status.st_ino)),
        ]
    }

    func makeAttachment(_ data: Data) throws -> ProcessAttachmentFixture {
        let owner = "process-owner"
        let conversation = "process-conversation"
        let token = UUID().uuidString.lowercased()
        let ownerComponent = processSHA256(Data(owner.utf8))
        let conversationComponent = processSHA256(Data(conversation.utf8))
        let directory = root
            .appendingPathComponent(ownerComponent, isDirectory: true)
            .appendingPathComponent(conversationComponent, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try data.write(to: directory.appendingPathComponent(token))
        return .init(
            data: data,
            relativePath: [ownerComponent, conversationComponent, token].joined(separator: "/"),
            arguments: [
                "owner_user_id": .string(owner),
                "conversation_id": .string(conversation),
                "token": .string(token),
                "display_name": .string("attachment.txt"),
                "media_type": .string("text/plain"),
                "byte_size": .number(Double(data.count)),
                "sha256": .string(processSHA256(data)),
            ]
        )
    }

    func dispose() { try? FileManager.default.removeItem(at: root) }
}

private struct ProcessAttachmentFixture {
    let data: Data
    let relativePath: String
    let arguments: [String: CodeReadWorkerJSONValue]
}

private enum ProcessFixtureError: Error { case invalid }

private func processSHA256(_ data: Data) -> String {
    SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
}
