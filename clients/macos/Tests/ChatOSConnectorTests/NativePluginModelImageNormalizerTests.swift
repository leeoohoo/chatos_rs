import ChatOSNetworking
import Foundation
import ImageIO
import Testing
@testable import ChatOSConnector

@Suite("Native Plugin Model Image Normalizer")
struct NativePluginModelImageNormalizerTests {
    @Test("PNG normalization waits for the shared heavy-media lane")
    func pngUsesHeavyMediaGovernor() async throws {
        let png = try #require(Data(base64Encoded:
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        ))
        let governor = ClientHeavyMediaWorkGovernor(limit: 1)
        let blocker = try await governor.acquire(context: .init(
            taskID: "blocker",
            kind: .videoFrameExtraction,
            estimatedDecodedBytes: 64 * 1_024 * 1_024
        ))
        let normalization = Task {
            try await NativePluginModelImageNormalizer.normalizeForModel(.object([
                "content": .array([.object([
                    "type": .string("image"),
                    "mimeType": .string("image/png"),
                    "data": .string(png.base64EncodedString()),
                ])]),
            ]), taskID: "plugin-call", heavyMediaGovernor: governor)
        }
        try await waitForHeavyMediaQueue(governor)
        let snapshot = await governor.snapshot()
        #expect(snapshot.queued == [.init(
            taskID: "plugin-image:plugin-call:normalize",
            kind: .imageDecode,
            estimatedDecodedBytes: 4_096 * 4_096 * 4
        )])

        await blocker.releaseAndWait()
        let result = try await normalization.value
        #expect(result.jsonObject?["content"]?.jsonArray?.first?
            .jsonObject?["mimeType"]?.jsonString == "image/jpeg")
        #expect(await governor.snapshot().active.isEmpty)
    }

    @Test("PNG image blocks become locally decoded JPEG blocks")
    func pngBecomesJPEG() async throws {
        let png = try #require(Data(base64Encoded:
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        ))
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
        ]))

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
        ]))

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

        let governor = ClientHeavyMediaWorkGovernor(limit: 1)
        let blocker = try await governor.acquire(context: .init(
            taskID: "blocker",
            kind: .imageDecode,
            estimatedDecodedBytes: 1
        ))
        #expect(try await NativePluginModelImageNormalizer.normalizeForModel(
            original,
            taskID: "jpeg",
            heavyMediaGovernor: governor
        ) == original)
        #expect(await governor.snapshot().queued.isEmpty)
        await blocker.releaseAndWait()
    }

    @Test("oversized encoded PNG is rejected before decoding")
    func oversizedPNGIsRejected() async throws {
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
        ]))

        let item = try #require(result.jsonObject?["content"]?.jsonArray?.first?.jsonObject)
        #expect(item["type"]?.jsonString == "text")
    }

    @Test("only a bounded number of image blocks are normalized")
    func imageBlockCountIsBounded() async throws {
        let png = try #require(Data(base64Encoded:
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
        ))
        let item = NativeJSONValue.object([
            "type": .string("image"),
            "mimeType": .string("image/png"),
            "data": .string(png.base64EncodedString()),
        ])
        let result = try await NativePluginModelImageNormalizer.normalizeForModel(.object([
            "content": .array(Array(
                repeating: item,
                count: NativePluginModelImageNormalizer.maximumImageBlocks + 2
            )),
        ]))

        let content = try #require(result.jsonObject?["content"]?.jsonArray)
        #expect(content.prefix(NativePluginModelImageNormalizer.maximumImageBlocks).allSatisfy {
            $0.jsonObject?["mimeType"]?.jsonString == "image/jpeg"
        })
        #expect(content.suffix(2).allSatisfy {
            $0.jsonObject?["type"]?.jsonString == "text"
        })
    }

    private func waitForHeavyMediaQueue(
        _ governor: ClientHeavyMediaWorkGovernor
    ) async throws {
        for _ in 0..<200 {
            if await governor.snapshot().queued.count == 1 { return }
            try await Task.sleep(for: .milliseconds(5))
        }
        Issue.record("Timed out waiting for heavy-media admission")
    }
}
