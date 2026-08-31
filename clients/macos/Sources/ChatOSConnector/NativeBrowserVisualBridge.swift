import Foundation

enum NativeBrowserVisualBridge {
    static func browserSessionID(
        arguments: NativeJSONValue,
        result: NativeJSONValue
    ) -> String? {
        findString(key: "browser_session_id", in: arguments)
            ?? findString(key: "browser_session_id", in: result)
            ?? findLabeledValue("browser_session_id", in: result)
    }

    static func captureFrame(
        from result: NativeJSONValue,
        artifactRootURL: URL
    ) -> Data? {
        if let data = findImageData(in: result) { return data }
        guard let relativePath = findArtifactPath(in: result) else { return nil }
        let root = artifactRootURL.standardizedFileURL.resolvingSymlinksInPath()
        let candidate = root.appendingPathComponent(relativePath).standardizedFileURL
        let prefix = root.path + "/"
        guard candidate.path.hasPrefix(prefix),
              let values = try? candidate.resourceValues(forKeys: [
                .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
              ]),
              values.isRegularFile == true,
              values.isSymbolicLink != true,
              (values.fileSize ?? 2 * 1_024 * 1_024 + 1) <= 2 * 1_024 * 1_024 else {
            return nil
        }
        return try? Data(contentsOf: candidate, options: .mappedIfSafe)
    }

    static func publish(
        frame: Data,
        adapterSessionID: String,
        visualSessionURL: URL,
        sequence: UInt64,
        target: String?
    ) throws {
        guard frame.count <= 2 * 1_024 * 1_024 else { return }
        try frame.write(
            to: visualSessionURL.appendingPathComponent("frame.png"),
            options: .atomic
        )
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        var metadata: [String: NativeJSONValue] = [
            "protocol_version": .number(1),
            "session_id": .string("browser-\(adapterSessionID)"),
            "status": .string("running"),
            "title": .string("浏览器操作"),
            "mime_type": .string("image/png"),
            "frame_file": .string("frame.png"),
            "frame_sequence": .number(Double(sequence)),
            "captured_at": .string(formatter.string(from: Date())),
        ]
        if let target, !target.isEmpty { metadata["target_app"] = .string(target) }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(NativeJSONValue.object(metadata)).write(
            to: visualSessionURL.appendingPathComponent("session.json"),
            options: .atomic
        )
    }

    static func targetDescription(arguments: NativeJSONValue, result: NativeJSONValue) -> String? {
        let raw = findString(key: "url", in: arguments) ?? findString(key: "url", in: result)
        guard let raw else { return "浏览器" }
        return URL(string: raw)?.host ?? raw
    }

    private static func findString(key: String, in value: NativeJSONValue) -> String? {
        switch value {
        case let .object(object):
            if let direct = object[key]?.jsonString, !direct.isEmpty { return direct }
            for child in object.values {
                if let found = findString(key: key, in: child) { return found }
            }
        case let .array(values):
            for child in values {
                if let found = findString(key: key, in: child) { return found }
            }
        case let .string(text):
            if let data = text.data(using: .utf8),
               let decoded = try? JSONDecoder().decode(NativeJSONValue.self, from: data),
               let found = findString(key: key, in: decoded) {
                return found
            }
        default:
            break
        }
        return nil
    }

    private static func findLabeledValue(_ label: String, in value: NativeJSONValue) -> String? {
        switch value {
        case let .string(text):
            guard let range = text.range(of: label) else { return nil }
            let suffix = text[range.upperBound...]
                .drop(while: { !$0.isLetter && !$0.isNumber })
            let value = suffix.prefix(while: {
                $0.isLetter || $0.isNumber || $0 == "-" || $0 == "_"
            })
            return value.isEmpty ? nil : String(value)
        case let .array(values):
            return values.lazy.compactMap { findLabeledValue(label, in: $0) }.first
        case let .object(object):
            return object.values.lazy.compactMap { findLabeledValue(label, in: $0) }.first
        default:
            return nil
        }
    }

    private static func findImageData(in value: NativeJSONValue) -> Data? {
        switch value {
        case let .object(object):
            if object["type"]?.jsonString == "image",
               let encoded = object["data"]?.jsonString,
               let data = Data(base64Encoded: encoded) {
                return data
            }
            return object.values.lazy.compactMap(findImageData).first
        case let .array(values):
            return values.lazy.compactMap(findImageData).first
        case let .string(text):
            guard let data = text.data(using: .utf8),
                  let decoded = try? JSONDecoder().decode(NativeJSONValue.self, from: data) else {
                return nil
            }
            return findImageData(in: decoded)
        default:
            return nil
        }
    }

    private static func findArtifactPath(in value: NativeJSONValue) -> String? {
        for key in ["relative_path", "artifact_path", "path"] {
            if let path = findString(key: key, in: value), path.lowercased().hasSuffix(".png") {
                return path
            }
        }
        return nil
    }
}
