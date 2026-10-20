@testable import ChatOSApp
import ChatOSCore
import Foundation
import XCTest

@MainActor
final class ProjectDirectoryMutationLifecycleTests: XCTestCase {
    func testLateSaveReloadCannotReplaceNewlySelectedFile() async {
        let service = DeferredProjectFilesystem()
        let model = ProjectDirectoryViewModel(
            projectID: "project",
            rootPath: "/project",
            service: service,
            codeNavigationService: MutationNoopCodeNavigation()
        )
        await model.load()
        let first = entry("/project/first.txt")
        let second = entry("/project/second.txt")
        await model.toggle(first)
        model.beginEditing()
        model.draft = "updated first"

        let save = Task { await model.save() }
        while !(await service.writeStarted) { await Task.yield() }
        await model.toggle(second)
        await service.finishWrite()
        await save.value

        XCTAssertEqual(model.selectedPath, second.path)
        XCTAssertEqual(model.selectedFile?.path, second.path)
        XCTAssertEqual(model.draft, "second")
        XCTAssertFalse(model.isSaving)
        XCTAssertFalse(model.isMutating)
    }

    func testCreateAndDeleteUseMutationResultToRefreshParent() async {
        let service = DeferredProjectFilesystem(suspendsWrites: false)
        let model = ProjectDirectoryViewModel(
            projectID: "project",
            rootPath: "/project",
            service: service,
            codeNavigationService: MutationNoopCodeNavigation()
        )
        await model.load()

        await model.create(name: "new.txt", directory: false)
        XCTAssertTrue(model.visibleEntries.contains(where: { $0.entry.path == "/project/new.txt" }))
        await model.toggle(entry("/project/new.txt"))
        await model.deleteSelected()

        XCTAssertFalse(model.visibleEntries.contains(where: { $0.entry.path == "/project/new.txt" }))
        XCTAssertNil(model.selectedPath)
        let calls = await service.mutationCalls
        XCTAssertEqual(calls, ["create-file:/project:new.txt", "delete:/project/new.txt:false"])
    }

    func testCommittedCreateIsNotReportedAsFailedWhenOnlyRefreshFails() async {
        let service = DeferredProjectFilesystem(suspendsWrites: false)
        let model = ProjectDirectoryViewModel(
            projectID: "project",
            rootPath: "/project",
            service: service,
            codeNavigationService: MutationNoopCodeNavigation()
        )
        await model.load()
        await service.setListingFailure(true)

        await model.create(name: "created.txt", directory: false)

        XCTAssertEqual(model.errorMessage, "项目文件已创建，但目录暂时无法刷新。")
        let containsFile = await service.containsFile("/project/created.txt")
        XCTAssertTrue(containsFile)
        XCTAssertFalse(model.isMutating)
    }

    private func entry(_ path: String) -> ProjectFileEntry {
        .init(
            name: URL(fileURLWithPath: path).lastPathComponent,
            path: path,
            displayPath: path,
            isDirectory: false,
            isWritable: true,
            size: 6
        )
    }
}

private actor DeferredProjectFilesystem: ProjectFilesystemServicing {
    private var files: [String: String] = [
        "/project/first.txt": "first",
        "/project/second.txt": "second",
    ]
    private let suspendsWrites: Bool
    private var writeContinuation: CheckedContinuation<Void, Never>?
    private var failsListing = false
    private(set) var writeStarted = false
    private(set) var mutationCalls: [String] = []

    init(suspendsWrites: Bool = true) {
        self.suspendsWrites = suspendsWrites
    }

    func listEntries(path: String, forceRefresh: Bool) async throws -> ProjectDirectoryListing {
        if failsListing { throw URLError(.cannotOpenFile) }
        let entries = files.keys
            .filter { parentPath($0) == path }
            .sorted()
            .map { path in
                ProjectFileEntry(
                    name: URL(fileURLWithPath: path).lastPathComponent,
                    path: path,
                    displayPath: path,
                    isDirectory: false,
                    isWritable: true,
                    size: Int64(files[path]?.utf8.count ?? 0)
                )
            }
        return .init(path: path, parentPath: nil, isWritable: true, entries: entries, isTruncated: false)
    }

    func searchEntries(path: String, query: String, limit: Int) async throws -> [ProjectFileEntry] { [] }
    func searchContent(path: String, query: String, limit: Int) async throws -> [ProjectFileContentMatch] { [] }

    func readFile(path: String) async throws -> ProjectFileContent {
        let content = files[path] ?? ""
        return .init(
            path: path,
            displayPath: path,
            name: URL(fileURLWithPath: path).lastPathComponent,
            contentType: "text/plain",
            isBinary: false,
            isWritable: true,
            size: Int64(content.utf8.count),
            modifiedAt: nil,
            content: content
        )
    }

    func writeFile(path: String, content: String) async throws {
        writeStarted = true
        if suspendsWrites {
            await withCheckedContinuation { continuation in
                writeContinuation = continuation
            }
        }
        files[path] = content
    }

    func finishWrite() {
        writeContinuation?.resume()
        writeContinuation = nil
    }

    func setListingFailure(_ value: Bool) {
        failsListing = value
    }

    func containsFile(_ path: String) -> Bool {
        files[path] != nil
    }

    func createFile(parentPath: String, name: String) async throws {
        mutationCalls.append("create-file:\(parentPath):\(name)")
        files[parentPath + "/" + name] = ""
    }

    func createDirectory(parentPath: String, name: String) async throws {
        mutationCalls.append("create-directory:\(parentPath):\(name)")
    }

    func deleteEntry(path: String, recursive: Bool) async throws {
        mutationCalls.append("delete:\(path):\(recursive)")
        files[path] = nil
    }

    func openExternally(path: String, mode: ProjectFileExternalOpenMode) async throws {}

    private func parentPath(_ path: String) -> String {
        URL(fileURLWithPath: path).deletingLastPathComponent().path
    }
}

private struct MutationNoopCodeNavigation: ProjectCodeNavigationServicing {
    func definition(
        projectRoot: String,
        filePath: String,
        line: Int,
        column: Int
    ) async throws -> ProjectCodeNavigationResult {
        .init(provider: "test", language: "test", mode: "definition", token: nil, locations: [])
    }

    func references(
        projectRoot: String,
        filePath: String,
        line: Int,
        column: Int
    ) async throws -> ProjectCodeNavigationResult {
        .init(provider: "test", language: "test", mode: "references", token: nil, locations: [])
    }
}
