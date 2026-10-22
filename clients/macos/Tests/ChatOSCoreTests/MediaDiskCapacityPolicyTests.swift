import Testing
@testable import ChatOSCore

@Suite("Media Disk Capacity Policy")
struct MediaDiskCapacityPolicyTests {
    @Test("accepts the exact write, retained copy, and free-space boundary")
    func acceptsExactBoundary() throws {
        let writeBytes: Int64 = 512
        let retainedBytes: Int64 = 256
        let reserve: Int64 = 128

        try MediaDiskCapacityPolicy.ensureCapacity(
            availableBytes: writeBytes + retainedBytes + reserve,
            writeBytes: writeBytes,
            retainedBytes: retainedBytes,
            minimumFreeBytes: reserve
        )
    }

    @Test("rejects one byte below the complete capacity budget")
    func rejectsInsufficientCapacity() {
        #expect(throws: MediaDiskCapacityError.self) {
            try MediaDiskCapacityPolicy.ensureCapacity(
                availableBytes: 895,
                writeBytes: 512,
                retainedBytes: 256,
                minimumFreeBytes: 128
            )
        }
    }

    @Test("rejects arithmetic overflow instead of wrapping the budget")
    func rejectsOverflow() {
        #expect(throws: MediaDiskCapacityError.self) {
            try MediaDiskCapacityPolicy.ensureCapacity(
                availableBytes: Int64.max,
                writeBytes: Int64.max,
                retainedBytes: 1,
                minimumFreeBytes: 0
            )
        }
    }
}
