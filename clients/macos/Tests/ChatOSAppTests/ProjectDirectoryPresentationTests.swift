import ChatOSCore
import XCTest
@testable import ChatOSApp

final class ProjectDirectoryPresentationTests: XCTestCase {
    func testConnectorPathHidesRoutingIDsAndDecodesFolderName() {
        let directory = ProjectDirectoryPresentation(
            "local://connector/device-id/grant-id/Users/lilei/Projects/%E4%B8%89%E5%9B%BD%20game/"
        )
        XCTAssertEqual(directory.name, "三国 game")
        XCTAssertEqual(directory.path, "Users/lilei/Projects/三国 game")
    }

    func testConnectorWithoutDirectoryDoesNotDisplayGrantID() {
        XCTAssertNil(ProjectDirectoryPresentation("local://connector/device-id/grant-id").name)
        XCTAssertNil(ProjectDirectoryPresentation("local://connector/device-id").path)
        XCTAssertNil(ProjectDirectoryPresentation("local://unknown/device-id/grant-id/project").path)
    }

    func testPlainPathPreservesLiteralPercentEscapes() {
        let directory = ProjectDirectoryPresentation(" /Volumes/Work/Project%20one/ ")
        XCTAssertEqual(directory.path, "/Volumes/Work/Project%20one/")
        XCTAssertEqual(directory.name, "Project%20one")
    }

    func testURIIsDecodedOnlyOnce() {
        let directory = ProjectDirectoryPresentation("local://connector/device/grant/Users/Project%2520one")
        XCTAssertEqual(directory.name, "Project%20one")
    }

    func testFileURLAndRootPath() {
        XCTAssertEqual(ProjectDirectoryPresentation("file:///Users/Shared/My%20Project/").name, "My Project")
        XCTAssertEqual(ProjectDirectoryPresentation("/").name, "/")
    }

    func testMissingDirectory() {
        XCTAssertNil(ProjectDirectoryPresentation(nil).path)
        XCTAssertNil(ProjectDirectoryPresentation(" \n ").name)
    }

    func testKnownWorkspaceResolvesDisplayPathWithoutAssumingRootGrant() {
        let workspace = LocalConnectorWorkspace(
            id: "grant", alias: "Work", absoluteRoot: "/Volumes/Work", fingerprint: "test"
        )
        let directory = ProjectDirectoryPresentation(
            "local://connector/device/grant/My%20Project", workspaces: [workspace]
        )
        XCTAssertEqual(directory.path, "/Volumes/Work/My Project")
        XCTAssertEqual(directory.name, "My Project")
        XCTAssertEqual(ProjectDirectoryPresentation(
            "local://connector/device/grant", workspaces: [workspace]
        ).name, "Work")
    }
}
