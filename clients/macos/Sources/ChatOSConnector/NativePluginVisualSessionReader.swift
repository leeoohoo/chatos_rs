import ChatOSCore
import Foundation

enum NativePluginVisualSessionReader {
    private static let maximumMetadataBytes = 16 * 1_024
    private static let maximumFrameBytes = 2 * 1_024 * 1_024

    private struct Host: Decodable {
        var protocolVersion: Int
        var adapterSessionID: String
        var pluginID: String
        var componentKey: String

        enum CodingKeys: String, CodingKey {
            case protocolVersion = "protocol_version"
            case adapterSessionID = "adapter_session_id"
            case pluginID = "plugin_id"
            case componentKey = "component_key"
        }
    }

    private struct Metadata: Decodable {
        var protocolVersion: Int
        var sessionID: String
        var status: String
        var title: String
        var targetApplication: String?
        var mimeType: String?
        var frameFile: String?
        var frameSequence: UInt64
        var capturedAt: String
        var width: Int?
        var height: Int?

        enum CodingKeys: String, CodingKey {
            case protocolVersion = "protocol_version"
            case sessionID = "session_id"
            case status, title
            case targetApplication = "target_app"
            case mimeType = "mime_type"
            case frameFile = "frame_file"
            case frameSequence = "frame_sequence"
            case capturedAt = "captured_at"
            case width, height
        }
    }

    static func read(
        descriptors: [NativePluginRuntimeStore.VisualDescriptor],
        now: Date = Date(),
        loadFrameDataForAdapterSessionIDs: Set<String>? = nil
    ) -> [PluginVisualSession] {
        descriptors.sorted { $0.ownerBoundAt > $1.ownerBoundAt }.compactMap { descriptor in
            read(
                descriptor: descriptor,
                now: now,
                loadFrameData: loadFrameDataForAdapterSessionIDs?.contains(
                    descriptor.identity.adapterSessionID
                ) ?? true
            )
        }
    }

    private static func read(
        descriptor: NativePluginRuntimeStore.VisualDescriptor,
        now: Date,
        loadFrameData: Bool
    ) -> PluginVisualSession? {
        guard let host: Host = boundedJSON(
            descriptor.visualSessionURL.appendingPathComponent("host.json")
        ),
              host.protocolVersion == 1,
              host.adapterSessionID == descriptor.identity.adapterSessionID,
              host.pluginID == descriptor.identity.pluginID,
              host.componentKey == descriptor.identity.componentKey else {
            return nil
        }
        guard let metadata: Metadata = boundedJSON(
            descriptor.visualSessionURL.appendingPathComponent("session.json")
        ) else { return nil }
        guard metadata.protocolVersion == 1,
              metadata.status == "running",
              safeLabel(metadata.sessionID, maximum: 256),
              safeLabel(metadata.title, maximum: 120),
              metadata.targetApplication.map({ safeLabel($0, maximum: 120) }) ?? true,
              let capturedAt = parseDate(metadata.capturedAt),
              capturedAt.timeIntervalSince(now) <= 5 else {
            return nil
        }
        guard let frameURL = validatedFrameURL(
            metadata: metadata,
            directory: descriptor.visualSessionURL
        ) else { return nil }
        let frame = loadFrameData
            ? try? Data(contentsOf: frameURL, options: .mappedIfSafe)
            : nil
        if loadFrameData, frame == nil { return nil }
        return PluginVisualSession(
            id: metadata.sessionID,
            adapterSessionID: descriptor.identity.adapterSessionID,
            pluginID: descriptor.identity.pluginID,
            componentKey: descriptor.identity.componentKey,
            pluginDisplayName: descriptor.displayName,
            title: metadata.title,
            targetApplication: metadata.targetApplication,
            frameSequence: metadata.frameSequence,
            capturedAt: capturedAt,
            frameData: frame,
            mimeType: metadata.mimeType,
            width: metadata.width,
            height: metadata.height,
            owner: descriptor.owner
        )
    }

    private static func boundedJSON<T: Decodable>(_ url: URL) -> T? {
        guard let values = try? url.resourceValues(forKeys: [
            .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
        ]),
              values.isRegularFile == true,
              values.isSymbolicLink != true,
              (values.fileSize ?? maximumMetadataBytes + 1) <= maximumMetadataBytes,
              let data = try? Data(contentsOf: url, options: .mappedIfSafe) else {
            return nil
        }
        return try? JSONDecoder().decode(T.self, from: data)
    }

    private static func validatedFrameURL(metadata: Metadata, directory: URL) -> URL? {
        let expectedFile: String
        switch metadata.mimeType {
        case "image/jpeg": expectedFile = "frame.jpg"
        case "image/png": expectedFile = "frame.png"
        default: return nil
        }
        guard metadata.frameFile == expectedFile else { return nil }
        let url = directory.appendingPathComponent(expectedFile)
        guard let values = try? url.resourceValues(forKeys: [
            .isRegularFileKey, .isSymbolicLinkKey, .fileSizeKey,
        ]),
              values.isRegularFile == true,
              values.isSymbolicLink != true,
              (values.fileSize ?? maximumFrameBytes + 1) <= maximumFrameBytes else {
            return nil
        }
        return url
    }

    private static func safeLabel(_ value: String, maximum: Int) -> Bool {
        !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && value.utf8.count <= maximum
            && !value.unicodeScalars.contains(where: CharacterSet.controlCharacters.contains)
    }

    private static func parseDate(_ value: String) -> Date? {
        let fractional = ISO8601DateFormatter()
        fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return fractional.date(from: value) ?? ISO8601DateFormatter().date(from: value)
    }
}
