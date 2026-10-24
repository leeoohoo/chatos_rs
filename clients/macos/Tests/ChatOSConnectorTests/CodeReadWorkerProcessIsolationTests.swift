import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
import CryptoKit
import Darwin
import Foundation
import XCTest

final class CodeReadWorkerProcessIsolationTests: XCTestCase {
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
        print -r -- '{"type":"hello_response","ok":true,"selected_protocol_version":1,"worker_build_version":"code-read-worker-v1","helper_instance_id":"fixture-helper","operations":["read_file_raw","read_file_range","list_dir","search_text","read_file","search_files","open_file_in_pet","project_list_v1","project_search_entries_v1","project_run_analyze_v1","project_write_file_v1","project_create_directory_v1","project_delete_entry_v1","attachment_resolve_v1","attachment_verify_file_v1","attachment_materialize_v1"],"maximum_inline_bytes":16777216,"error":null}'
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
