@testable import ChatOSConnector
import Foundation
import XCTest

final class NativeProjectRunAnalyzerTests: XCTestCase {
    func testDetectsNodeAndSwiftTargetsLocally() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-swift-run-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data(#"{"scripts":{"dev":"vite","test":"vitest"}}"#.utf8)
            .write(to: root.appendingPathComponent("package.json"))
        let swiftPackage = root.appendingPathComponent("Native", isDirectory: true)
        try FileManager.default.createDirectory(at: swiftPackage, withIntermediateDirectories: true)
        try Data("// swift-tools-version: 6.2\n".utf8)
            .write(to: swiftPackage.appendingPathComponent("Package.swift"))
        let executionCache = root.appendingPathComponent(".chatos/executions/run/frontend", isDirectory: true)
        try FileManager.default.createDirectory(at: executionCache, withIntermediateDirectories: true)
        try Data(#"{"scripts":{"start":"fake-cache-target"}}"#.utf8)
            .write(to: executionCache.appendingPathComponent("package.json"))

        let result = try NativeProjectRunAnalyzer().analyze(root: root)

        XCTAssertTrue(result.targets.contains(where: { $0.command == "npm run dev" }))
        XCTAssertTrue(result.targets.contains(where: { $0.command == "swift run" }))
        XCTAssertEqual(result.targets.first?.command, "npm run dev")
        XCTAssertTrue(result.configurationFiles.contains(where: { $0.path == "package.json" }))
        XCTAssertTrue(result.configurationFiles.contains(where: { $0.path == "Native/Package.swift" }))
        XCTAssertFalse(result.configurationFiles.contains(where: { $0.path.contains(".chatos") }))
    }
}
