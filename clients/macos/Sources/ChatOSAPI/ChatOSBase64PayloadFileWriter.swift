import ChatOSCore
import Foundation

/// Materializes a provider Base64 image as a lease-owned file while keeping decoded memory
/// bounded to one small chunk.
enum ChatOSBase64PayloadFileWriter {
    private static let decodedChunkQuartets = 64 * 1_024

    static func write(
        _ value: String,
        maximumBytes: Int,
        temporaryDirectory: URL = FileManager.default.temporaryDirectory,
        availableCapacityOverride: Int64? = nil
    ) async throws -> GeneratedMediaFile {
        guard let expectedBytes = ChatOSBase64PayloadValidator.decodedByteCount(
            value,
            maximumBytes: maximumBytes
        ) else {
            throw MediaGenerationClientError.invalidProviderResponse
        }
        do {
            return try await ChatOSResponseBodyFileWriter.write(
                DecodedChunks(value),
                maximumBytes: maximumBytes,
                expectedBytes: expectedBytes,
                temporaryDirectory: temporaryDirectory,
                availableCapacityOverride: availableCapacityOverride,
                artifactKind: .imageResponse
            )
        } catch MediaGenerationClientError.invalidVideoContent {
            throw MediaGenerationClientError.invalidProviderResponse
        }
    }

    private struct DecodedChunks: AsyncSequence {
        typealias Element = Data

        let value: String

        init(_ value: String) {
            self.value = value
        }

        func makeAsyncIterator() -> Iterator {
            .init(iterator: value.utf8.makeIterator())
        }

        struct Iterator: AsyncIteratorProtocol {
            var iterator: String.UTF8View.Iterator

            mutating func next() async throws -> Data? {
                try Task.checkCancellation()
                var output = Data()
                output.reserveCapacity(decodedChunkQuartets * 3)

                for _ in 0..<decodedChunkQuartets {
                    guard let first = iterator.next() else { break }
                    guard let second = iterator.next(),
                          let third = iterator.next(),
                          let fourth = iterator.next(),
                          let firstValue = decodedValue(first),
                          let secondValue = decodedValue(second) else {
                        throw MediaGenerationClientError.invalidProviderResponse
                    }
                    output.append((firstValue << 2) | (secondValue >> 4))
                    if third == 0x3D { // == padding
                        guard fourth == 0x3D else {
                            throw MediaGenerationClientError.invalidProviderResponse
                        }
                        continue
                    }
                    guard let thirdValue = decodedValue(third) else {
                        throw MediaGenerationClientError.invalidProviderResponse
                    }
                    output.append(((secondValue & 0x0F) << 4) | (thirdValue >> 2))
                    if fourth == 0x3D { continue }
                    guard let fourthValue = decodedValue(fourth) else {
                        throw MediaGenerationClientError.invalidProviderResponse
                    }
                    output.append(((thirdValue & 0x03) << 6) | fourthValue)
                }
                return output.isEmpty ? nil : output
            }
        }
    }

    private static func decodedValue(_ byte: UInt8) -> UInt8? {
        switch byte {
        case 0x41...0x5A: byte - 0x41
        case 0x61...0x7A: byte - 0x61 + 26
        case 0x30...0x39: byte - 0x30 + 52
        case 0x2B: 62
        case 0x2F: 63
        default: nil
        }
    }
}
