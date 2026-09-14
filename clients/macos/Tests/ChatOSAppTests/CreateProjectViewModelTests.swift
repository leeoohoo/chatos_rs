@testable import ChatOSApp
import ChatOSCore
import Foundation
import XCTest

@MainActor
final class CreateProjectViewModelTests: XCTestCase {
    func testCreatesWithoutDeviceContactGitOrNetwork() async throws {
        let creator = RecordingLocalCreator()
        let model = CreateProjectViewModel(creationService: creator)
        model.selectDirectory(FileManager.default.temporaryDirectory)
        model.updateProjectName(" Local ")
        XCTAssertTrue(model.canCreate)
        let result = await model.save()
        XCTAssertEqual(result?.name, "Local")
        XCTAssertNil(result?.latestConversationID)
        let drafts = await creator.drafts
        XCTAssertEqual(drafts, [.init(name: "Local", rootPath: FileManager.default.temporaryDirectory.resolvingSymlinksInPath().path)])
    }

    func testFailedDirectoryLoadCannotSaveAnUnselectedDirectory() async throws {
        let creator = RecordingLocalCreator()
        let model = CreateProjectViewModel(creationService: creator)
        model.updateProjectName("Local")
        XCTAssertFalse(model.canCreate)
        let result = await model.save()
        XCTAssertNil(result)
        let drafts = await creator.drafts
        XCTAssertTrue(drafts.isEmpty)
    }
}

private actor RecordingLocalCreator: LocalProjectCreating {
    var drafts: [LocalProjectDraft] = []
    func createProject(_ draft: LocalProjectDraft) async throws -> WorkspaceProject {
        drafts.append(draft)
        return .init(id: "new-local", name: draft.name, rootPath: "/test-workspace", latestConversationID: nil)
    }
}
