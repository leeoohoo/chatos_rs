import CryptoKit
import Foundation

struct NativeRelayVerifier: Sendable {
    func verify(
        _ request: NativeRelayRequest,
        trust: GatewayRemoteControlTrustDTO,
        ownerUserID: String,
        deviceID: String,
        seenNonces: inout [String: Int64]
    ) throws {
        guard request.ownerUserID == ownerUserID else {
            throw NativeRelayVerificationError.ownerMismatch
        }
        guard request.deviceID == deviceID else {
            throw NativeRelayVerificationError.deviceMismatch
        }

        let hasSignature = request.platformSignature != nil
            || request.platformSignatureKeyID != nil
            || request.platformSignatureAlgorithm != nil
            || request.platformTimestamp != nil
            || request.platformNonce != nil
        if !hasSignature {
            guard !trust.requireSignedMessages else {
                throw NativeRelayVerificationError.signatureRequired
            }
            return
        }
        guard request.platformSignatureAlgorithm == "ed25519",
              let keyID = request.platformSignatureKeyID?.trimmedNonEmpty,
              let publicKeyText = trust.trustedRelayPublicKeys[keyID],
              let timestamp = request.platformTimestamp,
              let nonce = request.platformNonce?.trimmedNonEmpty,
              (16...128).contains(nonce.count),
              let signatureText = request.platformSignature?.trimmedNonEmpty else {
            throw NativeRelayVerificationError.invalidSignatureMetadata
        }
        let now = Int64(Date().timeIntervalSince1970)
        guard abs(now - timestamp) <= Int64(trust.signatureMaxSkewSeconds) else {
            throw NativeRelayVerificationError.expiredSignature
        }
        seenNonces = seenNonces.filter { $0.value >= now - Int64(trust.signatureMaxSkewSeconds) }
        let nonceKey = "\(keyID):\(nonce)"
        guard seenNonces[nonceKey] == nil else {
            throw NativeRelayVerificationError.replayedNonce
        }
        let payload = try signaturePayload(request)
        guard let publicKeyData = decodeBase64URL(publicKeyText.removingPrefix("ed25519:")),
              let signature = decodeBase64URL(signatureText),
              let publicKey = try? Curve25519.Signing.PublicKey(rawRepresentation: publicKeyData),
              publicKey.isValidSignature(signature, for: payload) else {
            throw NativeRelayVerificationError.invalidSignature
        }
        seenNonces[nonceKey] = now
    }

    private func signaturePayload(_ request: NativeRelayRequest) throws -> Data {
        guard let keyID = request.platformSignatureKeyID,
              let algorithm = request.platformSignatureAlgorithm,
              let timestamp = request.platformTimestamp,
              let nonce = request.platformNonce else {
            throw NativeRelayVerificationError.invalidSignatureMetadata
        }
        let canonicalHeaders = NativeJSONValue.object(
            request.headers.mapValues(NativeJSONValue.string)
        ).canonicalJSONString
        let value = [
            "v1", request.type, request.requestID,
            request.ownerUserID ?? "", request.deviceID ?? "", request.workspaceID,
            request.method ?? "", request.path ?? "", keyID, algorithm,
            String(timestamp), nonce, canonicalHeaders, request.body.canonicalJSONString,
        ].joined(separator: "\n")
        return Data(value.utf8)
    }

    private func decodeBase64URL(_ value: String) -> Data? {
        var normalized = value.replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        normalized += String(repeating: "=", count: (4 - normalized.count % 4) % 4)
        return Data(base64Encoded: normalized)
    }
}

private enum NativeRelayVerificationError: LocalizedError {
    case ownerMismatch, deviceMismatch, signatureRequired, invalidSignatureMetadata
    case expiredSignature, replayedNonce, invalidSignature

    var errorDescription: String? {
        switch self {
        case .ownerMismatch: "Relay 用户身份与当前配对账号不一致"
        case .deviceMismatch: "Relay 设备身份与当前设备不一致"
        case .signatureRequired: "Relay 请求缺少平台签名"
        case .invalidSignatureMetadata: "Relay 签名元数据无效"
        case .expiredSignature: "Relay 签名已过期"
        case .replayedNonce: "Relay 请求 nonce 已被使用"
        case .invalidSignature: "Relay 平台签名验证失败"
        }
    }
}

private extension String {
    var trimmedNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }

    func removingPrefix(_ prefix: String) -> String {
        hasPrefix(prefix) ? String(dropFirst(prefix.count)) : self
    }
}
