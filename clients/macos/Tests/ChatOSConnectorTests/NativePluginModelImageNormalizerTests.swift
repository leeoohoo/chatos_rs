import Foundation
import ImageIO
import Testing
@testable import ChatOSConnector

@Suite("Native Plugin Model Image Normalizer")
struct NativePluginModelImageNormalizerTests {
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

        #expect(try await NativePluginModelImageNormalizer.normalizeForModel(original) == original)
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
}
