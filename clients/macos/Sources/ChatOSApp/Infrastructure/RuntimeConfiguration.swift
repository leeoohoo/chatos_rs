import ChatOSAPI
import Foundation

enum RuntimeConfiguration {
    struct Deployment: Sendable, Equatable {
        let identifier: String
        let apiBaseURL: URL
        let connectorBaseURL: URL
    }

    static let deployment = loadDeployment()

    static var apiBaseURL: URL { deployment.apiBaseURL }

    static var projectConversationID: String {
        nonEmptyEnvironmentValue("CHATOS_PROJECT_CONVERSATION_ID")
            ?? "conversation-test-project"
    }

    static var localConnectorCloudBaseURL: URL { deployment.connectorBaseURL }

    static var nativeConnectorStateURL: URL {
        let root = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first ?? FileManager.default.homeDirectoryForCurrentUser
        let appRoot = root.appendingPathComponent("ChatOSSwift", isDirectory: true)
        let deploymentRoot: URL
        if deployment.identifier == "production" {
            // Keep the released profile on the historical path so existing projects,
            // plugins and permissions remain available after this migration.
            deploymentRoot = appRoot
        } else {
            deploymentRoot = appRoot
                .appendingPathComponent("Environments", isDirectory: true)
                .appendingPathComponent(safePathComponent(deployment.identifier), isDirectory: true)
        }
        return deploymentRoot
            .appendingPathComponent("NativeConnector", isDirectory: true)
            .appendingPathComponent("state.json", isDirectory: false)
    }

    static var contactConversationID: String {
        nonEmptyEnvironmentValue("CHATOS_CONTACT_CONVERSATION_ID")
            ?? "conversation-contact"
    }

    static func attachmentURL(for value: String?) -> URL? {
        ChatOSAttachmentURLResolver.resolve(value, apiBaseURL: apiBaseURL)
    }

    private static func nonEmptyEnvironmentValue(_ key: String) -> String? {
        let value = ProcessInfo.processInfo.environment[key]?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return value?.isEmpty == false ? value : nil
    }

    private static func loadDeployment() -> Deployment {
        let requestedProfile = nonEmptyEnvironmentValue("CHATOS_DEPLOYMENT_PROFILE")
            ?? bundleString("ChatOSDeploymentProfile")
            ?? "local"
        if let profiles = Bundle.main.object(
            forInfoDictionaryKey: "ChatOSDeploymentProfiles"
        ) as? [String: Any],
           let value = profiles[requestedProfile] as? [String: Any],
           let apiBaseURL = validHTTPURL(value["APIBaseURL"]),
           let connectorBaseURL = validHTTPURL(value["ConnectorBaseURL"]) {
            return .init(
                identifier: requestedProfile,
                apiBaseURL: apiBaseURL,
                connectorBaseURL: connectorBaseURL
            )
        }
        if requestedProfile == "production" {
            return .init(
                identifier: "production",
                apiBaseURL: URL(string: "https://gateway.jgoool.com/api/chatos")!,
                connectorBaseURL: URL(string: "https://connector.jgoool.com")!
            )
        }
        return .init(
            identifier: "local",
            apiBaseURL: URL(string: "http://127.0.0.1:9080/api/chatos")!,
            connectorBaseURL: URL(string: "http://127.0.0.1:9080/api/connector")!
        )
    }

    private static func bundleString(_ key: String) -> String? {
        (Bundle.main.object(forInfoDictionaryKey: key) as? String)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .nonEmpty
    }

    private static func validHTTPURL(_ rawValue: Any?) -> URL? {
        guard let value = (rawValue as? String)?
            .trimmingCharacters(in: .whitespacesAndNewlines),
              let url = URL(string: value),
              ["http", "https"].contains(url.scheme?.lowercased() ?? ""),
              url.host != nil else {
            return nil
        }
        return url
    }

    private static func safePathComponent(_ value: String) -> String {
        let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-_."))
        let normalized = value.unicodeScalars.map { allowed.contains($0) ? String($0) : "-" }
            .joined()
        return normalized.isEmpty ? "default" : normalized
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
