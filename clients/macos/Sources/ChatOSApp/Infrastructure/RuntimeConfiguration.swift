import ChatOSAPI
import ChatOSConnector
import CryptoKit
import Darwin
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

    static func localAgentBootstrapSettings(
        accountID: String,
        deviceID: String
    ) -> NativeLocalAgentHostBootstrapSettings {
        let accountDirectory = localAgentRootURL
            .appendingPathComponent(accountDirectoryName(accountID), isDirectory: true)
        return NativeLocalAgentHostBootstrapSettings(
            executableURL: localAgentHostExecutableURL,
            accountID: accountID,
            deviceID: deviceID,
            runtimeDirectory: localAgentRuntimeDirectory(accountID),
            attachmentGrantDirectory: accountDirectory
                .appendingPathComponent("AttachmentGrants", isDirectory: true),
            platformStateDirectory: accountDirectory
                .appendingPathComponent("PlatformState", isDirectory: true),
            modelGatewayBaseURL: modelGatewayBaseURL,
            memoryEngineBaseURL: memoryEngineBaseURL,
            storage: .sqlite(
                databaseURL: accountDirectory.appendingPathComponent("Client.sqlite3"),
                encryptionSecretReference: NativeLocalAgentAccountSession
                    .sqliteEncryptionKeyReference
            )
        )
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

    private static var localAgentRootURL: URL {
        let support = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first ?? FileManager.default.homeDirectoryForCurrentUser
        return support
            .appendingPathComponent("ChatOSSwift", isDirectory: true)
            .appendingPathComponent("LocalAgent", isDirectory: true)
    }

    static var localAgentHostExecutableURL: URL {
        if let configured = nonEmptyEnvironmentValue("CHATOS_LOCAL_AGENT_HOST_EXECUTABLE") {
            return URL(fileURLWithPath: configured)
        }
        return Bundle.main.bundleURL
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("MacOS", isDirectory: true)
            .appendingPathComponent("chatos_local_agent_host", isDirectory: false)
    }

    private static var modelGatewayBaseURL: URL {
        environmentURL("CHATOS_MODEL_GATEWAY_BASE_URL")
            ?? bundleURL("ChatOSModelGatewayBaseURL")
            ?? serviceRootURL
    }

    private static var memoryEngineBaseURL: URL {
        environmentURL("CHATOS_MEMORY_ENGINE_BASE_URL")
            ?? bundleURL("ChatOSMemoryEngineBaseURL")
            ?? serviceRootURL
    }

    private static var serviceRootURL: URL {
        guard var components = URLComponents(
            url: apiBaseURL,
            resolvingAgainstBaseURL: false
        ) else { return apiBaseURL }
        let suffix = "/api/chatos"
        if components.path.hasSuffix(suffix) {
            components.path.removeLast(suffix.count)
        }
        return components.url ?? apiBaseURL
    }

    private static func accountDirectoryName(_ accountID: String) -> String {
        let digest = SHA256.hash(data: Data(accountID.utf8))
        return digest.map { String(format: "%02x", $0) }.joined()
    }

    /// Unix-domain socket paths are limited to `sockaddr_un.sun_path` on macOS.
    /// Keep only ephemeral process communication under a short, user-private
    /// runtime path; durable account data remains in Application Support.
    private static func localAgentRuntimeDirectory(_ accountID: String) -> URL {
        URL(fileURLWithPath: "/tmp", isDirectory: true)
            .appendingPathComponent("chatos-la-\(geteuid())", isDirectory: true)
            .appendingPathComponent(
                String(accountDirectoryName(accountID).prefix(32)),
                isDirectory: true
            )
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
