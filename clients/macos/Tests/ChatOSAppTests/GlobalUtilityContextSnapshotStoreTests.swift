import Foundation
import XCTest
@testable import ChatOSApp

final class GlobalUtilityContextSnapshotStoreTests: XCTestCase {
    func testSnapshotRoundTripUsesPrivateAtomicFile() async throws {
        let fixture = makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }

        let didWrite = await GlobalUtilityContextSnapshotStore.write(
            fixture.input,
            to: fixture.url
        )
        let loaded = await GlobalUtilityContextSnapshotStore.read(from: fixture.url)

        XCTAssertTrue(didWrite)
        XCTAssertEqual(loaded, fixture.snapshot)
        let attributes = try FileManager.default.attributesOfItem(atPath: fixture.url.path)
        let permissions = try XCTUnwrap(attributes[.posixPermissions] as? NSNumber)
        XCTAssertEqual(permissions.intValue & 0o777, 0o600)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(
            atPath: fixture.url.deletingLastPathComponent().path
        ).allSatisfy { !$0.hasPrefix(".chatos-global-context-") })
    }

    func testOversizedSnapshotPreservesLastValidFile() async throws {
        let fixture = makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let initialWrite = await GlobalUtilityContextSnapshotStore.write(
            fixture.input,
            to: fixture.url
        )
        XCTAssertTrue(initialWrite)
        let oversized = GlobalUtilityContextSnapshotInput(
            isEnglish: true,
            projects: [
                .init(
                    id: "oversized",
                    title: String(
                        repeating: "x",
                        count: GlobalUtilityContextSnapshotStore.maximumBytes
                    ),
                    subtitle: nil,
                    conversationID: nil,
                    contactName: nil
                ),
            ],
            contacts: []
        )

        let oversizedWrite = await GlobalUtilityContextSnapshotStore.write(
            oversized,
            to: fixture.url
        )
        let preserved = await GlobalUtilityContextSnapshotStore.read(from: fixture.url)
        XCTAssertFalse(oversizedWrite)
        XCTAssertEqual(preserved, fixture.snapshot)
    }

    func testSnapshotReadRejectsSymbolicLinkAndOversizedFile() async throws {
        let fixture = makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let directory = fixture.url.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let target = directory.appendingPathComponent("target.json")
        try Data("{}".utf8).write(to: target)
        try FileManager.default.createSymbolicLink(at: fixture.url, withDestinationURL: target)

        let linkedRead = await GlobalUtilityContextSnapshotStore.read(from: fixture.url)
        XCTAssertNil(linkedRead)

        try FileManager.default.removeItem(at: fixture.url)
        try Data(
            repeating: 0x78,
            count: GlobalUtilityContextSnapshotStore.maximumBytes + 1
        ).write(to: fixture.url)
        let oversizedRead = await GlobalUtilityContextSnapshotStore.read(from: fixture.url)
        XCTAssertNil(oversizedRead)
    }

    func testPersistenceCoalescesPendingSnapshotsToLatestValue() async throws {
        let fixture = makeFixture()
        defer { try? FileManager.default.removeItem(at: fixture.root) }
        let probe = GlobalUtilitySnapshotWriteProbe()
        let persistCount = LockedPersistCount()
        let persistence = GlobalUtilityContextSnapshotPersistence(
            url: fixture.url,
            writer: { snapshot, _ in await probe.write(snapshot) },
            didPersist: { persistCount.increment() }
        )
        let second = snapshotInput(id: "second")
        let latest = snapshotInput(id: "latest")

        await persistence.submit(fixture.input)
        try await waitUntil { await probe.snapshotCount() == 1 }
        await persistence.submit(second)
        await persistence.submit(latest)
        await probe.releaseFirstWrite()
        try await waitUntil {
            await probe.snapshotCount() == 2 && persistCount.value == 2
        }

        let recorded = await probe.recordedSnapshots()
        XCTAssertEqual(recorded, [fixture.input, latest])
        await persistence.stop()
    }

    private func makeFixture() -> (
        root: URL,
        url: URL,
        input: GlobalUtilityContextSnapshotInput,
        snapshot: GlobalUtilityContextSnapshot
    ) {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(
            "global-utility-context-\(UUID().uuidString)",
            isDirectory: true
        )
        let input = snapshotInput(id: "initial")
        return (root, root.appendingPathComponent("nested/context.json"), input, input.snapshot())
    }

    private func snapshotInput(id: String) -> GlobalUtilityContextSnapshotInput {
        .init(
            isEnglish: false,
            projects: [
                .init(
                    id: id,
                    title: "Project \(id)",
                    subtitle: "Local",
                    conversationID: nil,
                    contactName: nil
                ),
            ],
            contacts: [
                .init(
                    id: "contact-\(id)",
                    title: "Contact",
                    subtitle: nil,
                    conversationID: nil,
                    contactName: nil
                ),
            ]
        )
    }

    private func waitUntil(
        _ condition: @escaping @Sendable () async -> Bool
    ) async throws {
        for _ in 0..<100 {
            if await condition() { return }
            try await Task.sleep(for: .milliseconds(10))
        }
        XCTFail("Timed out waiting for context snapshot persistence")
    }
}

private actor GlobalUtilitySnapshotWriteProbe {
    private var snapshots: [GlobalUtilityContextSnapshotInput] = []
    private var firstWriteContinuation: CheckedContinuation<Void, Never>?

    func write(_ snapshot: GlobalUtilityContextSnapshotInput) async -> Bool {
        snapshots.append(snapshot)
        if snapshots.count == 1 {
            await withCheckedContinuation { continuation in
                firstWriteContinuation = continuation
            }
        }
        return true
    }

    func releaseFirstWrite() {
        firstWriteContinuation?.resume()
        firstWriteContinuation = nil
    }

    func snapshotCount() -> Int { snapshots.count }
    func recordedSnapshots() -> [GlobalUtilityContextSnapshotInput] { snapshots }
}

private final class LockedPersistCount: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0

    var value: Int {
        lock.lock()
        defer { lock.unlock() }
        return count
    }

    func increment() {
        lock.lock()
        count += 1
        lock.unlock()
    }
}
