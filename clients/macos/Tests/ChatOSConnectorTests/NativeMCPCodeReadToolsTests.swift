import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeMCPCodeReadToolsTests {
    @Test
    func bundledRipgrepIsAvailableWithoutShellPath() throws {
        let executable = try #require(NativeBundledToolLocator.executable(named: "rg"))

        #expect(FileManager.default.isExecutableFile(atPath: executable.path))
    }

    @Test
    func readsListsAndSearchesInsideProjectWithBundledRipgrep() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        try FileManager.default.createDirectory(
            at: fixture.root.appendingPathComponent("Sources", isDirectory: true),
            withIntermediateDirectories: true
        )
        try Data("struct SpaceStation {\n    let dockingCode = \"NOVA\"\n}\n".utf8)
            .write(to: fixture.root.appendingPathComponent("Sources/Station.swift"))
        try Data("# Nova Dreamer\n".utf8)
            .write(to: fixture.root.appendingPathComponent("README.md"))

        let tools = fixture.tools()
        let listed = try tools.call(
            name: "list_dir",
            arguments: ["path": .string(".")]
        )
        let entries = try listed.object().array("entries")
        #expect(entries.contains { (try? $0.object().string("name")) == "Sources" })

        let read = try tools.call(
            name: "read_file_raw",
            arguments: ["path": .string("README.md")]
        ).object()
        #expect(read.string("content") == "# Nova Dreamer\n")
        #expect(read.string("sha256")?.count == 64)

        let searched = try tools.call(
            name: "search_text",
            arguments: ["pattern": .string("dockingCode")]
        ).object()
        #expect(searched.number("count") == 1)
        let first = try #require(try searched.array("results").first?.object())
        #expect(first.string("path") == "Sources/Station.swift")
        #expect(first.number("line") == 2)
    }

    @Test
    func presentsAProjectFileInThePetWorkbench() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let file = fixture.root.appendingPathComponent("preview.png")
        try Data([0, 1, 2, 3]).write(to: file)
        let received = PetPresentationNotificationBox()
        let token = NotificationCenter.default.addObserver(
            forName: .chatOSPetOpenFileRequested,
            object: nil,
            queue: nil
        ) { notification in
            received.store(notification.object as? PetFilePresentationRequest)
        }
        defer { NotificationCenter.default.removeObserver(token) }

        let result = try fixture.tools().call(
            name: "open_file_in_pet",
            arguments: [
                "path": .string("preview.png"),
                "line": .number(12),
                "mode": .string("edit"),
            ]
        ).object()

        #expect(result.bool("presented") == true)
        #expect(result.string("path") == file.path)
        #expect(received.value == PetFilePresentationRequest(
            path: file.path,
            targetLine: 12,
            prefersEditing: true
        ))
    }

    @Test
    func defaultToolRootScopesRelativeToolPaths() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let backend = fixture.root.appendingPathComponent("backend", isDirectory: true)
        try FileManager.default.createDirectory(at: backend, withIntermediateDirectories: true)
        try Data("service-name = nova\n".utf8)
            .write(to: backend.appendingPathComponent("service.txt"))

        let read = try fixture.tools(defaultToolRoot: "backend").call(
            name: "read_file_raw",
            arguments: ["path": .string("service.txt")]
        ).object()

        #expect(read.string("path") == "backend/service.txt")
        #expect(read.string("content") == "service-name = nova\n")
    }

    @Test
    func workspaceScopedProjectAcceptsProjectAbsoluteAndWorkspaceRelativePaths() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let sources = fixture.root.appendingPathComponent("Sources", isDirectory: true)
        try FileManager.default.createDirectory(at: sources, withIntermediateDirectories: true)
        try Data("struct SpaceStation {}\n".utf8)
            .write(to: sources.appendingPathComponent("Station.swift"))

        let workspaceRoot = fixture.root.deletingLastPathComponent()
        let cwd = fixture.root.lastPathComponent
        let tools = fixture.tools(workspaceRoot: workspaceRoot, requestCWD: cwd)

        let listed = try tools.call(
            name: "list_dir",
            arguments: ["path": .string(fixture.root.path)]
        ).object()
        #expect(try listed.array("entries").contains {
            (try? $0.object().string("name")) == "Sources"
        })

        let read = try tools.call(
            name: "read_file_raw",
            arguments: ["path": .string("\(cwd)/Sources/Station.swift")]
        ).object()
        #expect(read.string("path") == "Sources/Station.swift")

        let searched = try tools.call(
            name: "search_text",
            arguments: [
                "pattern": .string("SpaceStation"),
                "path": .string(sources.path),
            ]
        ).object()
        #expect(searched.number("count") == 1)
    }

    @Test
    func workspaceRelativeDefaultToolRootIsNotDuplicatedUnderProjectRoot() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let backend = fixture.root.appendingPathComponent("backend", isDirectory: true)
        try FileManager.default.createDirectory(at: backend, withIntermediateDirectories: true)
        try Data("service-name = nova\n".utf8)
            .write(to: backend.appendingPathComponent("service.txt"))

        let workspaceRoot = fixture.root.deletingLastPathComponent()
        let cwd = fixture.root.lastPathComponent
        let read = try fixture.tools(
            workspaceRoot: workspaceRoot,
            requestCWD: cwd,
            defaultToolRoot: "\(cwd)/backend"
        ).call(
            name: "read_file_raw",
            arguments: ["path": .string("service.txt")]
        ).object()

        #expect(read.string("path") == "backend/service.txt")
    }

    @Test
    func projectAbsolutePathOutsideCurrentProjectRemainsRejected() throws {
        let fixture = try Fixture()
        defer { fixture.dispose() }
        let workspaceRoot = fixture.root.deletingLastPathComponent()
        let tools = fixture.tools(
            workspaceRoot: workspaceRoot,
            requestCWD: fixture.root.lastPathComponent
        )

        do {
            _ = try tools.call(
                name: "list_dir",
                arguments: ["path": .string(workspaceRoot.path)]
            )
            Issue.record("项目外绝对路径不应被读取")
        } catch {
            #expect(error.localizedDescription == "路径超出当前项目范围")
        }
    }

    @Test
    func configuredRealProjectSupportsAbsoluteReadPaths() throws {
        guard let configuredRoot = ProcessInfo.processInfo.environment["CHATOS_CODE_READ_E2E_PROJECT"]?
            .trimmingCharacters(in: .whitespacesAndNewlines),
              !configuredRoot.isEmpty else { return }
        let root = URL(fileURLWithPath: configuredRoot, isDirectory: true)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        let workspace = LocalConnectorWorkspace(
            id: "real-project-e2e",
            alias: "Real project E2E",
            absoluteRoot: "/",
            fingerprint: "real-project-e2e"
        )
        let tools = NativeMCPCodeReadTools(
            workspace: workspace,
            projectRoot: root,
            requestCWD: root.path.trimmingCharacters(in: CharacterSet(charactersIn: "/")),
            defaultToolRoot: nil
        )

        let listed = try tools.call(
            name: "list_dir",
            arguments: ["path": .string(root.path)]
        ).object()
        #expect(!(try listed.array("entries")).isEmpty)

        let searched = try tools.call(
            name: "search_text",
            arguments: [
                "pattern": .string("SpaceStationAutoConfiguration"),
                "path": .string(root.appendingPathComponent("src").path),
                "max_results": .number(20),
            ]
        ).object()
        #expect((searched.number("count") ?? 0) > 0)
    }
}

private struct Fixture {
    let root: URL
    let workspace: LocalConnectorWorkspace

    init() throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        workspace = LocalConnectorWorkspace(
            id: "workspace-1",
            alias: "Fixture",
            absoluteRoot: root.path,
            fingerprint: "fixture"
        )
    }

    func tools(
        workspaceRoot: URL? = nil,
        requestCWD: String? = nil,
        defaultToolRoot: String? = nil
    ) -> NativeMCPCodeReadTools {
        let scopedWorkspace = LocalConnectorWorkspace(
            id: workspace.id,
            alias: workspace.alias,
            absoluteRoot: (workspaceRoot ?? root).path,
            fingerprint: workspace.fingerprint
        )
        return NativeMCPCodeReadTools(
            workspace: scopedWorkspace,
            projectRoot: root,
            requestCWD: requestCWD,
            defaultToolRoot: defaultToolRoot
        )
    }

    func dispose() {
        try? FileManager.default.removeItem(at: root)
    }
}

private extension NativeJSONValue {
    func object() throws -> [String: NativeJSONValue] {
        guard case let .object(value) = self else { throw TestError.invalidShape }
        return value
    }
}

private extension Dictionary where Key == String, Value == NativeJSONValue {
    func string(_ key: String) -> String? {
        guard case let .string(value)? = self[key] else { return nil }
        return value
    }

    func number(_ key: String) -> Double? {
        guard case let .number(value)? = self[key] else { return nil }
        return value
    }

    func bool(_ key: String) -> Bool? {
        guard case let .bool(value)? = self[key] else { return nil }
        return value
    }

    func array(_ key: String) throws -> [NativeJSONValue] {
        guard case let .array(value)? = self[key] else { throw TestError.invalidShape }
        return value
    }
}

private enum TestError: Error {
    case invalidShape
}

private final class PetPresentationNotificationBox: @unchecked Sendable {
    private let lock = NSLock()
    private var storedValue: PetFilePresentationRequest?

    var value: PetFilePresentationRequest? {
        lock.withLock { storedValue }
    }

    func store(_ value: PetFilePresentationRequest?) {
        lock.withLock { storedValue = value }
    }
}
