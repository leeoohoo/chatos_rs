import Foundation
import Testing
@testable import ChatOSConnector

struct NativePluginModelResultCanonicalizerTests {
    @Test
    func preservesSmallCompatibilityTextSoModelCanReadPluginContext() {
        let structured: NativeJSONValue = .object([
            "scope": .object([
                "projectName": .string("my_blog"),
                "sourceModeHint": .string("existing-project"),
            ]),
            "workspace": .null,
        ])
        let result = NativeJSONValue.object([
            "content": .array([.object([
                "type": .string("text"),
                "text": .string(structured.canonicalJSONString),
            ])]),
            "structuredContent": structured,
        ])

        #expect(NativePluginModelResultCanonicalizer.canonicalize(result) == result)
    }

    @Test
    func removesTextThatDuplicatesStructuredContent() throws {
        let structured: NativeJSONValue = .object([
            "artifacts": .array([.object([
                "sha256": .string("abc"),
                "text": .string(String(repeating: "dom", count: 10_000)),
            ])]),
        ])
        let result = NativeJSONValue.object([
            "content": .array([.object([
                "type": .string("text"),
                "text": .string(structured.canonicalJSONString),
            ])]),
            "structuredContent": structured,
        ])

        let normalized = NativePluginModelResultCanonicalizer.canonicalize(result)

        #expect(normalized.jsonObject?["structuredContent"] == structured)
        #expect(
            normalized.jsonObject?["_meta"]?.jsonObject?["chatos/deduplicatedStructuredTextCopies"]
                == .number(1)
        )
        let text = try #require(
            normalized.jsonObject?["content"]?.jsonArray?.first?.jsonObject?["text"]?.jsonString
        )
        #expect(text.contains("Duplicate JSON text omitted"))
        #expect(text.utf8.count < 200)
    }

    @Test
    func preservesDistinctTextAndImageBlocks() {
        let result = NativeJSONValue.object([
            "content": .array([
                .object(["type": .string("text"), "text": .string("human summary")]),
                .object([
                    "type": .string("image"),
                    "mimeType": .string("image/jpeg"),
                    "data": .string("bytes"),
                ]),
            ]),
            "structuredContent": .object(["ok": .bool(true)]),
        ])

        #expect(NativePluginModelResultCanonicalizer.canonicalize(result) == result)
    }
}
