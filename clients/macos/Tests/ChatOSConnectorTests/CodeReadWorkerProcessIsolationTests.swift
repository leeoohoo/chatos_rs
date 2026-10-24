import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
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
        print -r -- '{"type":"hello_response","ok":true,"selected_protocol_version":1,"worker_build_version":"code-read-worker-v1","helper_instance_id":"fixture-helper","operations":["read_file_raw","read_file_range","list_dir","search_text","read_file","search_files","project_list_v1","project_search_entries_v1","project_run_analyze_v1"],"maximum_inline_bytes":16777216,"error":null}'
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

    func dispose() { try? FileManager.default.removeItem(at: root) }
}
