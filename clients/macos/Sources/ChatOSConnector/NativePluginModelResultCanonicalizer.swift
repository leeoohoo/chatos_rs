import Foundation

/// Removes the MCP compatibility text copy when it is only a JSON encoding of
/// `structuredContent`. The structured value remains the canonical model-visible
/// result, while images and genuinely distinct text blocks are preserved.
enum NativePluginModelResultCanonicalizer {
    /// Small compatibility text copies are intentionally kept. Agent tool outcomes are
    /// transported as a single string, so some model providers attend to MCP `content`
    /// more reliably than to the sibling `structuredContent` field. Deduplicating a tiny
    /// result saves almost nothing and can make an otherwise valid result look empty.
    static let maximumInlineCompatibilityTextCharacters = 8_000

    static func canonicalize(_ result: NativeJSONValue) -> NativeJSONValue {
        guard case var .object(root) = result,
              let structured = root["structuredContent"],
              case let .array(content)? = root["content"] else {
            return result
        }
        if structured.canonicalJSONString.count <= maximumInlineCompatibilityTextCharacters {
            return result
        }

        var removedCount = 0
        let retained = content.filter { item in
            guard item.jsonObject?["type"]?.jsonString == "text",
                  let text = item.jsonObject?["text"]?.jsonString,
                  let data = text.data(using: .utf8),
                  let decoded = try? JSONDecoder().decode(NativeJSONValue.self, from: data),
                  decoded == structured else {
                return true
            }
            removedCount += 1
            return false
        }
        guard removedCount > 0 else { return result }

        var normalizedContent = retained
        if normalizedContent.isEmpty {
            normalizedContent = [.object([
                "type": .string("text"),
                "text": .string(
                    "[Duplicate JSON text omitted; use structuredContent as the canonical result.]"
                ),
            ])]
        }
        root["content"] = .array(normalizedContent)
        var metadata = root["_meta"]?.jsonObject ?? [:]
        metadata["chatos/deduplicatedStructuredTextCopies"] = .number(Double(removedCount))
        root["_meta"] = .object(metadata)
        return .object(root)
    }
}
