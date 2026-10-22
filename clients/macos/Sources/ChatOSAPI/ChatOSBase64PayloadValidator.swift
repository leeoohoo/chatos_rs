import Foundation

/// Validates provider Base64 without materializing a decoded copy that callers immediately
/// discard. The actual image consumer still performs the single decode it needs.
enum ChatOSBase64PayloadValidator {
    static func decodedByteCount(_ value: String, maximumBytes: Int) -> Int? {
        guard maximumBytes >= 0 else { return nil }
        let bytes = value.utf8
        let encodedCount = bytes.count
        guard encodedCount >= 4, encodedCount.isMultiple(of: 4) else { return nil }

        var paddingCount = 0
        var reachedPadding = false
        for (index, byte) in bytes.enumerated() {
            if byte == 0x3D { // =
                guard index >= encodedCount - 2, paddingCount < 2 else { return nil }
                paddingCount += 1
                reachedPadding = true
            } else {
                guard !reachedPadding, isAlphabetByte(byte) else { return nil }
            }
        }

        let (maximumDecodedCount, overflow) = (encodedCount / 4)
            .multipliedReportingOverflow(by: 3)
        guard !overflow else { return nil }
        let decodedCount = maximumDecodedCount - paddingCount
        guard decodedCount > 0, decodedCount <= maximumBytes else { return nil }
        return decodedCount
    }

    private static func isAlphabetByte(_ byte: UInt8) -> Bool {
        switch byte {
        case 0x41...0x5A, 0x61...0x7A, 0x30...0x39, 0x2B, 0x2F:
            true
        default:
            false
        }
    }
}
