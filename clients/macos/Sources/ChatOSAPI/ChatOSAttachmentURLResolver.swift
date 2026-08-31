import Foundation

public enum ChatOSAttachmentURLResolver {
    public static func resolve(_ value: String?, apiBaseURL: URL) -> URL? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else { return nil }
        if let absolute = URL(string: value), absolute.scheme != nil {
            return absolute
        }

        if let relative = URLComponents(string: value),
           relative.path.hasPrefix("/api/attachments/") {
            guard var gateway = URLComponents(
                url: apiBaseURL,
                resolvingAgainstBaseURL: false
            ) else { return nil }
            let slash = CharacterSet(charactersIn: "/")
            let basePath = gateway.path.trimmingCharacters(in: slash)
            let attachmentPath = relative.path
                .dropFirst("/api".count)
                .trimmingCharacters(in: slash)
            gateway.path = "/" + [basePath, attachmentPath]
                .filter { !$0.isEmpty }
                .joined(separator: "/")
            gateway.percentEncodedQuery = relative.percentEncodedQuery
            gateway.percentEncodedFragment = relative.percentEncodedFragment
            return gateway.url
        }

        guard var origin = URLComponents(
            url: apiBaseURL,
            resolvingAgainstBaseURL: false
        ) else { return nil }
        origin.path = ""
        origin.query = nil
        origin.fragment = nil
        guard let originURL = origin.url else { return nil }
        return URL(string: value, relativeTo: originURL)?.absoluteURL
    }
}
