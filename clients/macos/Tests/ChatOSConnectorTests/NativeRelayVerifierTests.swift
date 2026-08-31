@testable import ChatOSConnector
import CryptoKit
import Foundation
import XCTest

final class NativeRelayVerifierTests: XCTestCase {
    func testVerifiesRustCompatibleSignatureWithoutEscapedPathSeparators() throws {
        let privateKey = Curve25519.Signing.PrivateKey()
        let keyID = "relay-key-1"
        let timestamp = Int64(Date().timeIntervalSince1970)
        let nonce = "12345678-1234-1234-1234-123456789012"
        let body = NativeJSONValue.object([
            "include_files": .bool(false),
            "path": .string("/Users/lilei/project/java"),
        ])
        let request = NativeRelayRequest(
            type: "workspace_directory_list_request",
            requestID: "request-1",
            ownerUserID: "user-1",
            deviceID: "device-1",
            workspaceID: "workspace-1",
            method: "POST",
            path: "/workspace/directory/list",
            headers: [:],
            body: body,
            platformSignature: nil,
            platformSignatureKeyID: keyID,
            platformSignatureAlgorithm: "ed25519",
            platformTimestamp: timestamp,
            platformNonce: nonce
        )
        let payload = [
            "v1", request.type, request.requestID, "user-1", "device-1", "workspace-1",
            "POST", "/workspace/directory/list", keyID, "ed25519", String(timestamp), nonce,
            "{}", #"{"include_files":false,"path":"/Users/lilei/project/java"}"#,
        ].joined(separator: "\n")
        var signedRequest = request
        signedRequest.platformSignature = base64URL(try privateKey.signature(for: Data(payload.utf8)))
        let trust = GatewayRemoteControlTrustDTO(
            requireSignedMessages: true,
            signatureMaxSkewSeconds: 300,
            trustedRelayPublicKeys: [
                keyID: "ed25519:" + base64URL(privateKey.publicKey.rawRepresentation),
            ]
        )
        var seenNonces: [String: Int64] = [:]

        XCTAssertNoThrow(try NativeRelayVerifier().verify(
            signedRequest,
            trust: trust,
            ownerUserID: "user-1",
            deviceID: "device-1",
            seenNonces: &seenNonces
        ))
    }

    private func base64URL(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}
