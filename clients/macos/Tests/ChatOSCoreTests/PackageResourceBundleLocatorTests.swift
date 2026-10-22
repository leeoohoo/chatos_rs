import Foundation
import ChatOSModelCapabilities
@testable import ChatOSCore
import XCTest

final class PackageResourceBundleLocatorTests: XCTestCase {
    private let resourceNames = [
        "ChatOSSwift_ChatOSCore.bundle",
        "ChatOSSwift_ChatOSModelCapabilities.bundle",
    ]

    private func temporaryDirectory() throws -> URL {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("package-resource-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        addTeardownBlock { try FileManager.default.removeItem(at: root) }
        return root
    }

    private func makeContainer(at url: URL) throws -> Bundle {
        let contents = url.appendingPathComponent("Contents", isDirectory: true)
        try FileManager.default.createDirectory(
            at: contents.appendingPathComponent("Resources"), withIntermediateDirectories: true
        )
        let plist = try PropertyListSerialization.data(fromPropertyList: [
            "CFBundleIdentifier": "com.chatos.resource-test.\(UUID().uuidString)",
            "CFBundlePackageType": url.pathExtension == "app" ? "APPL" : "BNDL",
        ], format: .xml, options: 0)
        try plist.write(to: contents.appendingPathComponent("Info.plist"))
        return try XCTUnwrap(Bundle(url: url))
    }

    private func makeResource(at url: URL) throws {
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        try Data("fixture resource".utf8).write(to: url.appendingPathComponent("fixture.txt"))
        XCTAssertNotNil(Bundle(url: url))
    }

    func testPackagedAppUsesResourcesNotSiblingBundles() throws {
        let root = try temporaryDirectory()
        let app = try makeContainer(at: root.appendingPathComponent("ChatOS.app"))
        let resources = try XCTUnwrap(app.resourceURL)
        for name in resourceNames {
            let expected = resources.appendingPathComponent(name)
            try makeResource(at: expected)
            try makeResource(at: root.appendingPathComponent(name))
            let result = ChatOSPackageResourceBundleLocator.locate(named: name, in: [app])
            XCTAssertEqual(result?.bundleURL.standardizedFileURL, expected.standardizedFileURL)
        }
    }

    func testPackagedAppMissingResourcesCannotUseBuildDirectorySibling() throws {
        let root = try temporaryDirectory()
        let app = try makeContainer(at: root.appendingPathComponent("ChatOS.app"))
        for name in resourceNames {
            try makeResource(at: root.appendingPathComponent(name))
            XCTAssertNil(ChatOSPackageResourceBundleLocator.locate(named: name, in: [app]))
        }
    }

    func testReleaseXCTestUsesSiblingResourceBundles() throws {
        let root = try temporaryDirectory()
        let tests = try makeContainer(at: root.appendingPathComponent("PackageTests.xctest"))
        for name in resourceNames {
            let expected = root.appendingPathComponent(name)
            try makeResource(at: expected)
            let result = ChatOSPackageResourceBundleLocator.locate(named: name, in: [tests])
            XCTAssertEqual(result?.bundleURL.standardizedFileURL, expected.standardizedFileURL)
        }
    }

    func testContainedResourcesTakePriorityOverXCTestSiblings() throws {
        let root = try temporaryDirectory()
        let tests = try makeContainer(at: root.appendingPathComponent("PackageTests.xctest"))
        let app = try makeContainer(at: root.appendingPathComponent("ChatOS.app"))
        let resources = try XCTUnwrap(app.resourceURL)
        for name in resourceNames {
            try makeResource(at: root.appendingPathComponent(name))
            let expected = resources.appendingPathComponent(name)
            try makeResource(at: expected)
            let result = ChatOSPackageResourceBundleLocator.locate(named: name, in: [tests, app])
            XCTAssertEqual(result?.bundleURL.standardizedFileURL, expected.standardizedFileURL)
        }
    }

    func testOrdinaryBundleDoesNotEnableXCTestFallback() throws {
        let root = try temporaryDirectory()
        let ordinary = try makeContainer(at: root.appendingPathComponent("Plugin.bundle"))
        for name in resourceNames {
            try makeResource(at: root.appendingPathComponent(name))
            XCTAssertNil(ChatOSPackageResourceBundleLocator.locate(named: name, in: [ordinary]))
        }
    }

    func testMissingXCTestResourcesAreNotInvented() throws {
        let root = try temporaryDirectory()
        let tests = try makeContainer(at: root.appendingPathComponent("PackageTests.xctest"))
        for name in resourceNames {
            XCTAssertNil(ChatOSPackageResourceBundleLocator.locate(named: name, in: [tests]))
        }
    }

    func testActualResourcesLoadUnderCurrentBuildConfiguration() throws {
        let bundle = ChatOSCoreResourceBundle.bundle
        XCTAssertNotNil(bundle.url(forResource: "AgentPrompt.Action.Default", withExtension: "md"))
        let capabilities = try XCTUnwrap(ModelReasoningCapabilities.forModel("gpt-6.1-sol"))
        XCTAssertFalse(capabilities.levels.isEmpty)
        XCTAssertFalse(LocalAgentPromptCatalog.render(.agentDefaultRole).isEmpty)
    }
}
