import ChatOSCore
import CryptoKit
import Foundation

enum NativePluginHash {
    static func sha256(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }

    static func canonicalSHA256(_ value: NativeJSONValue) throws -> String {
        try canonicalSHA256Encoded(value)
    }

    static func localAgentCanonicalSHA256(_ value: LocalAgentJSONValue) throws -> String {
        try canonicalSHA256Encoded(value)
    }

    private static func canonicalSHA256Encoded(_ value: some Encodable) throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        return sha256(try encoder.encode(value))
    }
}
