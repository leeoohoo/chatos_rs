import Foundation

enum NativeComputerUseVisualBridge {
    struct Frame: Equatable {
        var data: Data
        var mimeType: String
        var fileName: String
    }

    private static let maximumFrameBytes = 2 * 1_024 * 1_024
    private static let pngSignature = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
    private static let jpegSignature = Data([0xFF, 0xD8, 0xFF])

    static func captureFrame(from result: NativeJSONValue) -> Frame? {
        findImage(in: result)
    }

    static func targetApplication(arguments: NativeJSONValue) -> String? {
        let object = arguments.jsonObject
        guard let target = ["app", "app_name", "bundle_id"]
            .lazy
            .compactMap({ object?[$0]?.jsonString })
            .map({ $0.trimmingCharacters(in: .whitespacesAndNewlines) })
            .first(where: { !$0.isEmpty }),
              !target.isEmpty,
              target.utf8.count <= 120 else {
            return nil
        }
        return target
    }

    static func publish(
        frame: Frame,
        adapterSessionID: String,
        visualSessionURL: URL,
        sequence: UInt64,
        targetApplication: String?
    ) throws {
        guard isValid(frame) else { return }
        try frame.data.write(
            to: visualSessionURL.appendingPathComponent(frame.fileName),
            options: .atomic
        )
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        var metadata: [String: NativeJSONValue] = [
            "protocol_version": .number(1),
            "session_id": .string("computer-\(adapterSessionID)"),
            "status": .string("running"),
            "title": .string("电脑操作"),
            "mime_type": .string(frame.mimeType),
            "frame_file": .string(frame.fileName),
            "frame_sequence": .number(Double(sequence)),
            "captured_at": .string(formatter.string(from: Date())),
        ]
        if let targetApplication { metadata["target_app"] = .string(targetApplication) }
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(NativeJSONValue.object(metadata)).write(
            to: visualSessionURL.appendingPathComponent("session.json"),
            options: .atomic
        )
    }

    private static func findImage(in value: NativeJSONValue) -> Frame? {
        switch value {
        case let .object(object):
            if object["type"]?.jsonString == "image",
               let mimeType = object["mimeType"]?.jsonString?.lowercased(),
               let encoded = object["data"]?.jsonString,
               let data = Data(base64Encoded: encoded),
               data.count <= maximumFrameBytes {
                let frame: Frame?
                switch mimeType {
                case "image/png":
                    frame = Frame(data: data, mimeType: mimeType, fileName: "frame.png")
                case "image/jpeg", "image/jpg":
                    frame = Frame(data: data, mimeType: "image/jpeg", fileName: "frame.jpg")
                default:
                    frame = nil
                }
                if let frame, isValid(frame) { return frame }
            }
            return object.values.lazy.compactMap(findImage).first
        case let .array(values):
            return values.lazy.compactMap(findImage).first
        case let .string(text):
            guard let data = text.data(using: .utf8),
                  let decoded = try? JSONDecoder().decode(NativeJSONValue.self, from: data) else {
                return nil
            }
            return findImage(in: decoded)
        default:
            return nil
        }
    }

    private static func isValid(_ frame: Frame) -> Bool {
        guard frame.data.count <= maximumFrameBytes else { return false }
        switch frame.mimeType {
        case "image/png":
            return frame.fileName == "frame.png" && frame.data.starts(with: pngSignature)
        case "image/jpeg":
            return frame.fileName == "frame.jpg" && frame.data.starts(with: jpegSignature)
        default:
            return false
        }
    }
}
