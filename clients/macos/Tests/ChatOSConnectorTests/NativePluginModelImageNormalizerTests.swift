import ChatOSMediaWorkerClient
import ChatOSMediaWorkerProtocol
import Foundation
import ImageIO
import Testing
@testable import ChatOSConnector

@Suite("Native Plugin Model Image Normalizer")
struct NativePluginModelImageNormalizerTests {
    @Test("independent PNG blocks run through media workers concurrently")
    func pngBlocksRunConcurrently() async throws {
        let png = try testPNG()
        let item = NativeJSONValue.object([
            "type": .string("image"),
            "mimeType": .string("image/png"),
            "data": .string(png.base64EncodedString()),
        ])
        let worker = JPEGRecordingWorker()
        let result = try await NativePluginModelImageNormalizer.normalizeForModel(
            .object(["content": .array(Array(repeating: item, count: 3))]),
            mediaWorker: worker
        )

        #expect(result.jsonObject?["content"]?.jsonArray?.allSatisfy {
            $0.jsonObject?["mimeType"]?.jsonString == "image/jpeg"
        } == true)
        let snapshot = await worker.snapshot()
        #expect(snapshot.callCount == 3)
        #expect(snapshot.maximumConcurrent == 3)
    }

    @Test("PNG image blocks become locally decoded JPEG blocks")
    func pngBecomesJPEG() async throws {
        let png = try testPNG()
        let result = try await NativePluginModelImageNormalizer.normalizeForModel(.object([
            "content": .array([
                .object(["type": .string("text"), "text": .string("state")]),
                .object([
                    "type": .string("image"),
                    "mimeType": .string("image/png"),
                    "data": .string(png.base64EncodedString()),
                ]),
            ]),
            "isError": .bool(false),
        ]), mediaWorker: JPEGRecordingWorker())

        let image = try #require(result.jsonObject?["content"]?.jsonArray?.last?.jsonObject)
        #expect(image["mimeType"]?.jsonString == "image/jpeg")
        let encodedJPEG = try #require(image["data"]?.jsonString)
        let jpeg = try #require(Data(base64Encoded: encodedJPEG))
        #expect(jpeg.starts(with: [0xFF, 0xD8]))
        #expect(jpeg.suffix(2) == Data([0xFF, 0xD9]))
        let source = try #require(CGImageSourceCreateWithData(jpeg as CFData, nil))
        #expect(CGImageSourceCreateImageAtIndex(source, 0, nil) != nil)
    }

    @Test("invalid PNG blocks become explicit text failures")
    func invalidPNGDoesNotPoisonModelRequest() async throws {
        let result = try await NativePluginModelImageNormalizer.normalizeForModel(.object([
            "content": .array([
                .object([
                    "type": .string("image"),
                    "mimeType": .string("image/png"),
                    "data": .string(Data("not an image".utf8).base64EncodedString()),
                ]),
            ]),
        ]), mediaWorker: JPEGRecordingWorker())

        let item = try #require(result.jsonObject?["content"]?.jsonArray?.first?.jsonObject)
        #expect(item["type"]?.jsonString == "text")
        #expect(item["text"]?.jsonString?.contains("could not be decoded locally") == true)
    }

    @Test("existing JPEG image blocks pass through unchanged")
    func jpegPassesThrough() async throws {
        let original = NativeJSONValue.object([
            "content": .array([
                .object([
                    "type": .string("image"),
                    "mimeType": .string("image/jpeg"),
                    "data": .string("/9j/2Q=="),
                ]),
            ]),
        ])
        let worker = JPEGRecordingWorker()

        #expect(try await NativePluginModelImageNormalizer.normalizeForModel(
            original,
            mediaWorker: worker
        ) == original)
        #expect(await worker.snapshot().callCount == 0)
    }

    @Test("oversized encoded PNG is rejected before decoding")
    func oversizedPNGIsRejected() async throws {
        let worker = JPEGRecordingWorker()
        let result = try await NativePluginModelImageNormalizer.normalizeForModel(.object([
            "content": .array([
                .object([
                    "type": .string("image"),
                    "mimeType": .string("image/png"),
                    "data": .string(String(
                        repeating: "A",
                        count: NativePluginModelImageNormalizer.maximumEncodedCharacters + 1
                    )),
                ]),
            ]),
        ]), mediaWorker: worker)

        let item = try #require(result.jsonObject?["content"]?.jsonArray?.first?.jsonObject)
        #expect(item["type"]?.jsonString == "text")
        #expect(await worker.snapshot().callCount == 0)
    }

    @Test("only a bounded number of image blocks are normalized")
    func imageBlockCountIsBounded() async throws {
        let png = try testPNG()
        let item = NativeJSONValue.object([
            "type": .string("image"),
            "mimeType": .string("image/png"),
            "data": .string(png.base64EncodedString()),
        ])
        let worker = JPEGRecordingWorker()
        let result = try await NativePluginModelImageNormalizer.normalizeForModel(.object([
            "content": .array(Array(
                repeating: item,
                count: NativePluginModelImageNormalizer.maximumImageBlocks + 2
            )),
        ]), mediaWorker: worker)

        let content = try #require(result.jsonObject?["content"]?.jsonArray)
        #expect(content.prefix(NativePluginModelImageNormalizer.maximumImageBlocks).allSatisfy {
            $0.jsonObject?["mimeType"]?.jsonString == "image/jpeg"
        })
        #expect(content.suffix(2).allSatisfy {
            $0.jsonObject?["type"]?.jsonString == "text"
        })
        #expect(await worker.snapshot().callCount
            == NativePluginModelImageNormalizer.maximumImageBlocks)
    }

    private func testPNG() throws -> Data {
        try #require(Data(base64Encoded:
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        ))
    }
}

private actor JPEGRecordingWorker: MediaWorkerJPEGNormalizing {
    private var callCount = 0
    private var active = 0
    private var maximumConcurrent = 0

    func snapshot() -> (callCount: Int, maximumConcurrent: Int) {
        (callCount, maximumConcurrent)
    }

    func normalizeJPEG(
        sourceURL: URL,
        sourceIdentity: MediaWorkerFileIdentity?,
        destinationURL: URL,
        ownerUserID: String,
        maximumSourceBytes: Int,
        maximumSourcePixelCount: Int,
        maximumOutputPixelSize: Int,
        maximumOutputBytes: Int,
        compressionQuality: Double
    ) async throws -> MediaWorkerImageDescriptor {
        callCount += 1
        active += 1
        maximumConcurrent = max(maximumConcurrent, active)
        defer { active -= 1 }
        try await Task.sleep(for: .milliseconds(25))

        let data = try Data(contentsOf: sourceURL)
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            throw CocoaError(.fileReadCorruptFile)
        }
        let encoded = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(
            encoded,
            "public.jpeg" as CFString,
            1,
            nil
        ) else {
            throw CocoaError(.fileWriteUnknown)
        }
        CGImageDestinationAddImage(destination, image, [
            kCGImageDestinationLossyCompressionQuality: compressionQuality,
        ] as CFDictionary)
        guard CGImageDestinationFinalize(destination),
              encoded.length > 0,
              encoded.length <= maximumOutputBytes else {
            throw CocoaError(.fileWriteUnknown)
        }
        let output = Data(referencing: encoded)
        try output.write(to: destinationURL)
        return .init(
            path: destinationURL.path,
            mimeType: "image/jpeg",
            byteCount: output.count,
            sha256: String(repeating: "0", count: 64),
            pixelWidth: image.width,
            pixelHeight: image.height
        )
    }
}
