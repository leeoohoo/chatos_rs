@testable import ChatOSApp
import ChatOSCore
import Foundation
import XCTest

@MainActor
final class CreateProjectViewModelTests: XCTestCase {
    func testDirectoryLoadWithoutResolvedRootDoesNotReportOperationFailure() async {
        let model = ProjectDirectoryViewModel(
            projectID: "new-project",
            rootPath: nil,
            service: Directories(),
            codeNavigationService: NoopCodeNavigation()
        )

        await model.load()

        XCTAssertNil(model.errorMessage)
        XCTAssertFalse(model.isLoading)
    }

    func testCreatesWithoutDeviceContactGitOrNetwork() async throws {
        let creator = RecordingLocalCreator()
        let model = CreateProjectViewModel(connectorStatus: status, filesystemService: Directories(), creationService: creator)
        await model.loadInitialDirectory()
        model.updateProjectName(" Local ")
        XCTAssertTrue(model.canCreate)
        let result = await model.save()
        XCTAssertEqual(result?.name, "Local")
        XCTAssertNil(result?.latestConversationID)
        XCTAssertNil(model.errorMessage)
        XCTAssertFalse(model.isSaving)
        let drafts = await creator.drafts
        XCTAssertEqual(drafts, [.init(name: "Local", workspaceID: "ws")])
    }

    func testFailedDirectoryLoadCannotSaveAnUnselectedDirectory() async throws {
        let creator = RecordingLocalCreator()
        let model = CreateProjectViewModel(connectorStatus: status, filesystemService: Directories(fails: true), creationService: creator)
        await model.loadInitialDirectory()
        model.updateProjectName("Local")
        XCTAssertFalse(model.canCreate)
        let result = await model.save()
        XCTAssertNil(result)
        let drafts = await creator.drafts
        XCTAssertTrue(drafts.isEmpty)
    }

    private var status: LocalConnectorStatus {
        .init(configured: false, connectorRunning: false, developerMode: false, cloudBaseURL: nil,
              userServiceBaseURL: nil, deviceID: nil, deviceName: nil, user: nil, defaultWorkspaceID: "ws",
              workspaces: [.init(id: "ws", alias: "workspace", absoluteRoot: "/test-workspace", fingerprint: "fp")])
    }
}

private struct NoopCodeNavigation: ProjectCodeNavigationServicing {
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

private actor RecordingLocalCreator: LocalProjectCreating {
    var drafts: [LocalProjectDraft] = []
    func createProject(_ draft: LocalProjectDraft) async throws -> WorkspaceProject {
        drafts.append(draft)
        return .init(id: "new-local", name: draft.name, rootPath: "/test-workspace", latestConversationID: nil)
    }
}

private struct Directories: ProjectFilesystemServicing {
    var fails = false
    func listEntries(path: String, forceRefresh: Bool) async throws -> ProjectDirectoryListing {
        if fails { throw URLError(.noPermissionsToReadFile) }
        return .init(path: path, parentPath: nil, isWritable: true, entries: [], isTruncated: false)
    }
    func searchEntries(path: String, query: String, limit: Int) async throws -> [ProjectFileEntry] { [] }
    func searchContent(path: String, query: String, limit: Int) async throws -> [ProjectFileContentMatch] { [] }
    func readFile(path: String) async throws -> ProjectFileContent { throw URLError(.fileDoesNotExist) }
    func writeFile(path: String, content: String) async throws { throw URLError(.unsupportedURL) }
    func createFile(parentPath: String, name: String) async throws { throw URLError(.unsupportedURL) }
    func createDirectory(parentPath: String, name: String) async throws { throw URLError(.unsupportedURL) }
    func deleteEntry(path: String, recursive: Bool) async throws { throw URLError(.unsupportedURL) }
    func openExternally(path: String, mode: ProjectFileExternalOpenMode) async throws { throw URLError(.unsupportedURL) }
}
