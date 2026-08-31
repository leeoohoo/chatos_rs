import CryptoKit
import Foundation
import Testing
@testable import ChatOSConnector

struct NativeConnectorDeviceAuthenticationTests {
    @Test
    func connectionPayloadAndSignatureMatchGatewayContract() throws {
        let privateKey = try Curve25519.Signing.PrivateKey(
            rawRepresentation: Data(0..<32)
        )
        let identity = NativeConnectorDeviceIdentity(privateKey: privateKey)
        let payload = NativeConnectorDeviceAuthentication.connectionPayload(
            deviceID: "device-123",
            timestamp: "1787616000",
            nonce: "nonce-456",
            path: "/api/local-connectors/devices/device-123/connect"
        )

        #expect(
            String(decoding: payload, as: UTF8.self)
                == "v1\ndevice-123\n1787616000\nnonce-456\n/api/local-connectors/devices/device-123/connect"
        )

        let encodedSignature = try identity.signature(for: payload)
        let signature = try #require(Data(base64URLString: encodedSignature))
        #expect(privateKey.publicKey.isValidSignature(signature, for: payload))
        #expect(identity.publicKey.hasPrefix("ed25519:"))
        #expect(!encodedSignature.contains("="))
    }
}

private extension Data {
    init?(base64URLString: String) {
        var value = base64URLString
            .replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        value.append(String(repeating: "=", count: (4 - value.count % 4) % 4))
        self.init(base64Encoded: value)
    }
}
