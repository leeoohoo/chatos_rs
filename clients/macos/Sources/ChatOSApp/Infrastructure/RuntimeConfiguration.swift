import ChatOSAPI
import Foundation

enum RuntimeConfiguration {
    static var apiBaseURL: URL {
        environmentURL("CHATOS_API_BASE_URL")
            ?? bundleURL("ChatOSAPIBaseURL")
            ?? URL(string: "http://127.0.0.1:9080/api/chatos")!
    }

    static var projectConversationID: String {
        nonEmptyEnvironmentValue("CHATOS_PROJECT_CONVERSATION_ID")
            ?? "conversation-test-project"
    }

    static var localConnectorCloudBaseURL: URL {
        environmentURL("CHATOS_LOCAL_CONNECTOR_CLOUD_BASE_URL")
            ?? bundleURL("ChatOSLocalConnectorCloudBaseURL")
            ?? URL(string: "http://127.0.0.1:39230")!
    }

    static var nativeConnectorStateURL: URL {
        let root = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first ?? FileManager.default.homeDirectoryForCurrentUser
        return root
            .appendingPathComponent("ChatOSSwift", isDirectory: true)
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

    private static func environmentURL(_ key: String) -> URL? {
        nonEmptyEnvironmentValue(key).flatMap(URL.init(string:))
    }

    private static func bundleURL(_ key: String) -> URL? {
        (Bundle.main.object(forInfoDictionaryKey: key) as? String)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .nonEmpty
            .flatMap(URL.init(string:))
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
