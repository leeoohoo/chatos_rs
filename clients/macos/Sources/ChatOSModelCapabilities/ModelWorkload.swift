import Foundation

/// Operation profiles, independent of image *input* support and task-list preferences.
/// User Service embeds this same resource when importing and resolving models.
public enum ModelWorkload: String, Codable, Sendable {
    case chat, image, video

    private struct Catalog: Decodable {
        let imagePrefixes: [String]
        let videoPrefixes: [String]
    }

    private static let catalog: Catalog = {
        guard let url = ChatOSModelCapabilitiesResourceBundle.bundle.url(
            forResource: "ModelWorkloads", withExtension: "json"
        ), let data = try? Data(contentsOf: url),
              let catalog = try? JSONDecoder().decode(Catalog.self, from: data) else {
            preconditionFailure("Missing or invalid model workload resource")
        }
        return catalog
    }()

    public static func forModel(_ name: String) -> Self {
        let name = name.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if catalog.imagePrefixes.contains(where: name.hasPrefix) { return .image }
        if catalog.videoPrefixes.contains(where: name.hasPrefix) { return .video }
        return .chat
    }
}
