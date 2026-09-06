import ChatOSCore
import Foundation
import Testing
@testable import ChatOSApp

@MainActor
@Suite("Pet file workbench")
struct PetFileWorkbenchStoreTests {
    @Test("resolves project file links and preserves line targets")
    func resolvesProjectFileLinks() throws {
        let absolute = try #require(URL(string: "/Volumes/Work/App.swift:42"))
        #expect(PetFileLinkResolver.resolve(absolute, projectRootPath: nil) == .init(
            path: "/Volumes/Work/App.swift",
            targetLine: 42
        ))

        let relative = try #require(URL(string: "Sources/App.swift:17:4"))
        #expect(PetFileLinkResolver.resolve(
            relative,
            projectRootPath: "/Volumes/Work"
        ) == .init(
            path: "/Volumes/Work/Sources/App.swift",
            targetLine: 17
        ))

        let logical = try #require(URL(string: "docs/README.md#L9"))
        #expect(PetFileLinkResolver.resolve(
            logical,
            projectRootPath: "local://connector/device/workspace"
        ) == .init(
            path: "local://connector/device/workspace/docs/README.md",
            targetLine: 9
        ))

        let web = try #require(URL(string: "https://example.com/file.swift"))
        #expect(PetFileLinkResolver.resolve(web, projectRootPath: "/Volumes/Work") == nil)
    }

    @Test("opens multiple files and reuses an existing tab")
    func opensMultipleTabs() async throws {
        let service = MockPetProjectFilesystem(files: [
            "/workspace/a.swift": makeFile(path: "/workspace/a.swift", content: "let a = 1\n"),
            "/workspace/b.md": makeFile(path: "/workspace/b.md", content: "# B\n"),
        ])
        let store = PetFileWorkbenchStore(service: service)

        store.open(.init(path: "/workspace/a.swift", targetLine: 4))
        try await waitUntilReady(store, path: "/workspace/a.swift")
        store.open(.init(path: "/workspace/b.md"))
        try await waitUntilReady(store, path: "/workspace/b.md")
        store.open(.init(path: "/workspace/a.swift", targetLine: 8))

        #expect(store.tabs.count == 2)
        #expect(store.selectedTabID == "/workspace/a.swift")
        #expect(store.selectedTab?.targetLine == 8)
        #expect(store.isPresented)
    }

    @Test("uses direct local access only for Finder-selected files")
    func routesFinderSelectedFilesToLocalService() async throws {
        let path = "/Users/example/Desktop/example.swift"
        let workspaceService = MockPetProjectFilesystem(files: [
            path: makeFile(path: path, content: "workspace copy\n"),
        ])
        let localService = MockPetProjectFilesystem(files: [
            path: makeFile(path: path, content: "finder copy\n"),
        ])
        let store = PetFileWorkbenchStore(
            service: workspaceService,
            localFileService: localService
        )

        store.open(.init(path: path, access: .userSelectedLocal))
        try await waitUntilReady(store, path: path)

        #expect(store.selectedTab?.draft == "finder copy\n")
        #expect(store.selectedTab?.access == .userSelectedLocal)
    }

    @Test("reads and edits arbitrary code and configuration extensions")
    func readsArbitraryLocalTextFiles() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        let service = NativePetLocalFileService()
        let fixtures = [
            ("WebDesignApp.jsx", "export default function App() { return <main /> }\n"),
            ("settings.toml", "theme = \"dark\"\n"),
            ("vector.svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>\n"),
            ("Dockerfile", "FROM swift:latest\n"),
        ]

        for (name, content) in fixtures {
            let url = directory.appendingPathComponent(name)
            try content.write(to: url, atomically: true, encoding: .utf8)

            let opened = try await service.readFile(path: url.path)
            #expect(opened.content == content)
            #expect(!opened.isBinary)
            #expect(opened.isWritable)

            let edited = content + "# edited\n"
            try await service.writeFile(path: url.path, content: edited)
            #expect(try String(contentsOf: url, encoding: .utf8) == edited)
        }
    }

    @Test("protects a dirty tab when it is closed")
    func protectsDirtyTab() async throws {
        let path = "/workspace/main.swift"
        let service = MockPetProjectFilesystem(files: [
            path: makeFile(path: path, content: "let value = 1\n"),
        ])
        let store = PetFileWorkbenchStore(service: service)
        store.open(.init(path: path, mode: .edit))
        try await waitUntilReady(store, path: path)

        store.updateDraft("let value = 2\n")
        store.requestCloseTab(path)

        #expect(store.pendingCloseRequest?.target == .tab(path))
        #expect(store.tabs.count == 1)

        store.discardPendingClose()
        #expect(store.tabs.isEmpty)
        #expect(!store.isPresented)
    }

    @Test("saves text changes and clears the dirty marker")
    func savesChanges() async throws {
        let path = "/workspace/main.swift"
        let service = MockPetProjectFilesystem(files: [
            path: makeFile(path: path, content: "let value = 1\n"),
        ])
        let store = PetFileWorkbenchStore(service: service)
        store.open(.init(path: path, mode: .edit))
        try await waitUntilReady(store, path: path)
        store.updateDraft("let value = 2\n")

        let saved = await store.save()

        #expect(saved)
        #expect(store.selectedTab?.isDirty == false)
        #expect(await service.content(at: path) == "let value = 2\n")
    }

    @Test("detects an external change before overwriting")
    func detectsSaveConflict() async throws {
        let path = "/workspace/main.swift"
        let service = MockPetProjectFilesystem(files: [
            path: makeFile(path: path, content: "let value = 1\n"),
        ])
        let store = PetFileWorkbenchStore(service: service)
        store.open(.init(path: path, mode: .edit))
        try await waitUntilReady(store, path: path)
        store.updateDraft("let value = 2\n")
        await service.replaceContent(at: path, with: "let value = 3\n")

        let saved = await store.save()

        #expect(!saved)
        #expect(store.saveConflict?.tabID == path)
        #expect(await service.content(at: path) == "let value = 3\n")

        await store.overwriteConflictingFile()
        #expect(store.saveConflict == nil)
        #expect(await service.content(at: path) == "let value = 2\n")
        #expect(store.selectedTab?.isDirty == false)
    }

    private func waitUntilReady(
        _ store: PetFileWorkbenchStore,
        path: String
    ) async throws {
        for _ in 0..<100 {
            if let tab = store.tabs.first(where: { $0.path == path }),
               tab.loadState == .ready {
                return
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        Issue.record("Timed out waiting for \(path) to load")
    }

    private func makeFile(path: String, content: String) -> ProjectFileContent {
        ProjectFileContent(
            path: path,
            displayPath: path,
            name: URL(fileURLWithPath: path).lastPathComponent,
            contentType: "text/plain",
            isBinary: false,
            isWritable: true,
            size: Int64(content.utf8.count),
            modifiedAt: Date(timeIntervalSince1970: 1),
            content: content
        )
    }
}

private actor MockPetProjectFilesystem: ProjectFilesystemServicing {
    private var files: [String: ProjectFileContent]

    init(files: [String: ProjectFileContent]) {
        self.files = files
    }

    func content(at path: String) -> String? {
        files[path]?.content
    }

    func replaceContent(at path: String, with content: String) {
        guard var file = files[path] else { return }
        file.content = content
        file.size = Int64(content.utf8.count)
        file.modifiedAt = Date()
        files[path] = file
    }

    func listEntries(path: String, forceRefresh: Bool) async throws -> ProjectDirectoryListing {
        .init(path: path, parentPath: nil, isWritable: true, entries: [], isTruncated: false)
    }

    func searchEntries(path: String, query: String, limit: Int) async throws -> [ProjectFileEntry] {
        []
    }

    func searchContent(path: String, query: String, limit: Int) async throws -> [ProjectFileContentMatch] {
        []
    }

    func readFile(path: String) async throws -> ProjectFileContent {
        guard let file = files[path] else { throw MockFilesystemError.missingFile }
        return file
    }

    func writeFile(path: String, content: String) async throws {
        guard var file = files[path] else { throw MockFilesystemError.missingFile }
        file.content = content
        file.size = Int64(content.utf8.count)
        file.modifiedAt = Date()
        files[path] = file
    }

    func createFile(parentPath: String, name: String) async throws {}
    func createDirectory(parentPath: String, name: String) async throws {}
    func deleteEntry(path: String, recursive: Bool) async throws {}
    func openExternally(path: String, mode: ProjectFileExternalOpenMode) async throws {}
}

private enum MockFilesystemError: Error {
    case missingFile
}
