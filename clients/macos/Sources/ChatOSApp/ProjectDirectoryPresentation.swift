import ChatOSCore
import Foundation

/// Display-only projection. Never use the shortened path for filesystem operations.
struct ProjectDirectoryPresentation: Equatable {
    let path: String?

    var name: String? {
        guard let path else { return nil }
        return path.split(separator: "/").last.map(String.init) ?? path
    }

    init(_ rawPath: String?, workspaces: [LocalConnectorWorkspace] = []) {
        guard let value = rawPath?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            path = nil
            return
        }
        if let components = URLComponents(string: value),
           components.scheme?.lowercased() == "local" {
            // Connector and grant IDs are routing metadata, not directory names.
            let parts = components.path.split(separator: "/")
            guard components.host?.lowercased() == "connector", parts.count >= 2 else {
                path = nil
                return
            }
            let relativePath = parts.dropFirst(2).joined(separator: "/")
            if let workspace = workspaces.first(where: { $0.id == String(parts[1]) }) {
                path = relativePath.isEmpty
                    ? workspace.absoluteRoot
                    : URL(fileURLWithPath: workspace.absoluteRoot)
                        .appendingPathComponent(relativePath).path
            } else {
                // Without a known grant root, do not invent an absolute filesystem path.
                path = relativePath.isEmpty ? nil : relativePath
            }
        } else if let url = URL(string: value), url.isFileURL {
            path = url.path
        } else {
            // Plain filesystem paths may contain literal percent escapes.
            path = value
        }
    }
}
