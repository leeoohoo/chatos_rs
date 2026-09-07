import Foundation
import Testing
@testable import ChatOSConnector

struct NativePluginInstallerTests {
    @Test("tar output larger than a pipe buffer does not deadlock plugin validation")
    func largeArchiveListingCompletes() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("NativePluginInstaller-\(UUID().uuidString)", isDirectory: true)
        let package = root.appendingPathComponent("source/package", isDirectory: true)
        let archive = root.appendingPathComponent("plugin.tgz")
        try FileManager.default.createDirectory(at: package, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        for index in 0..<2_500 {
            let name = String(format: "file-%05d-%@", index, String(repeating: "x", count: 32))
            #expect(FileManager.default.createFile(
                atPath: package.appendingPathComponent(name).path,
                contents: Data()
            ))
        }

        let create = Process()
        create.executableURL = URL(fileURLWithPath: "/usr/bin/tar")
        create.arguments = ["-czf", archive.path, "-C", root.appendingPathComponent("source").path, "package"]
        create.standardOutput = FileHandle.nullDevice
        create.standardError = FileHandle.nullDevice
        try create.run()
        create.waitUntilExit()
        #expect(create.terminationStatus == 0)

        let installer = NativePluginInstaller(rootURL: root.appendingPathComponent("plugins"))
        let output = try installer.runTar(["-tzf", archive.path])
        #expect(output.utf8.count > 65_536)
        #expect(output.split(whereSeparator: \.isNewline).count == 2_501)
    }
}
