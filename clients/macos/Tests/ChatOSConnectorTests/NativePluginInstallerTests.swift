import Darwin
import Foundation
import Testing
@testable import ChatOSConnector

struct NativePluginInstallerTests {
    @Test("legacy installation is attested only against a trusted package snapshot")
    func legacyInstallationAttestationUsesTrustedSnapshot() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("NativePluginAttestation-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let manifest = root.appendingPathComponent("chatos.plugin.json")
        try Data("trusted".utf8).write(to: manifest)
        let trusted = try NativePluginInstallationIntegrity.snapshot(
            installationURL: root,
            maximumFiles: 10,
            maximumBytes: 1_024
        )
        let record = NativeInstalledPluginRecord(
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "1.0.0",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: root.path,
            installedAt: "2026-10-06T00:00:00Z"
        )
        let installer = NativePluginInstaller(rootURL: root.deletingLastPathComponent())

        let attested = try installer.attestLegacyInstallation(
            record: record,
            trustedPackageFileSHA256: trusted
        )
        #expect(attested.packageFileSHA256 == trusted)
        try NativePluginInstallationIntegrity.verify(record: attested, installationURL: root)

        try Data("tampered".utf8).write(to: manifest, options: .atomic)
        #expect(throws: NativeConnectorError.self) {
            _ = try installer.attestLegacyInstallation(
                record: record,
                trustedPackageFileSHA256: trusted
            )
        }
    }

    @Test("installed plugin files are revalidated before every launch")
    func installedFileTamperingIsRejected() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("NativePluginIntegrity-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let executable = root.appendingPathComponent("bin/plugin")
        try FileManager.default.createDirectory(
            at: executable.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try Data("original".utf8).write(to: executable)
        let checksums = try NativePluginInstallationIntegrity.snapshot(
            installationURL: root,
            maximumFiles: 10,
            maximumBytes: 1_024
        )
        let record = NativeInstalledPluginRecord(
            pluginID: "plugin-1",
            releaseID: "release-1",
            version: "1.0.0",
            artifactSHA256: String(repeating: "a", count: 64),
            installationPath: root.path,
            installedAt: "2026-10-06T00:00:00Z",
            packageFileSHA256: checksums
        )
        try NativePluginInstallationIntegrity.verify(record: record, installationURL: root)
        try Data("tampered".utf8).write(to: executable)
        #expect(throws: NativePluginRuntimeError.self) {
            try NativePluginInstallationIntegrity.verify(record: record, installationURL: root)
        }
    }

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

        do {
            _ = try installer.runTar(
                ["-tzf", archive.path],
                maximumOutputBytes: 32 * 1_024
            )
            Issue.record("expected oversized tar output to be rejected")
        } catch {
            #expect(error.localizedDescription.contains("tar 输出超过安全限制"))
        }
    }

    @Test("tar validation timeout terminates a blocked process")
    func blockedArchiveValidationTimesOut() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("NativePluginInstaller-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let fifo = root.appendingPathComponent("blocked.tgz")
        #expect(mkfifo(fifo.path, 0o600) == 0)
        let installer = NativePluginInstaller(rootURL: root.appendingPathComponent("plugins"))
        let startedAt = ContinuousClock.now

        do {
            _ = try installer.runTar(["-tzf", fifo.path], timeout: 0.05)
            Issue.record("expected blocked tar validation to time out")
        } catch {
            #expect(error.localizedDescription.contains("tar 执行超时"))
        }
        #expect(ContinuousClock.now - startedAt < .seconds(3))
    }
}
