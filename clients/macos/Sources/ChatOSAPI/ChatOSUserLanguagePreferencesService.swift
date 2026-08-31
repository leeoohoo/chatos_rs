import ChatOSCore
import Foundation

public struct ChatOSUserLanguagePreferencesService: UserLanguagePreferencesServicing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func fetch() async throws -> UserLanguagePreferences {
        let response: UserSettingsResponseDTO = try await client.request("/user-settings")
        return response.languagePreferences
    }

    public func update(
        userID: String,
        preferences: UserLanguagePreferences
    ) async throws -> UserLanguagePreferences {
        let body = try JSONEncoder().encode(UserSettingsUpdateDTO(
            userID: userID,
            settings: .init(preferences)
        ))
        let response: UserSettingsResponseDTO = try await client.request(
            "/user-settings",
            method: "PUT",
            body: body
        )
        return response.languagePreferences
    }
}

private struct UserSettingsResponseDTO: Decodable, Sendable {
    var settings: LanguageSettingsDTO?
    var effective: LanguageSettingsDTO?

    var languagePreferences: UserLanguagePreferences {
        (effective ?? settings ?? LanguageSettingsDTO()).domainModel
    }
}

private struct UserSettingsUpdateDTO: Encodable {
    var userID: String
    var settings: LanguageSettingsDTO

    enum CodingKeys: String, CodingKey {
        case userID = "user_id"
        case settings
    }
}

private struct LanguageSettingsDTO: Codable, Sendable {
    var interfaceLocale: String?
    var internalContextLocale: String?

    enum CodingKeys: String, CodingKey {
        case interfaceLocale = "UI_LOCALE"
        case internalContextLocale = "INTERNAL_CONTEXT_LOCALE"
    }

    init() {}

    init(_ preferences: UserLanguagePreferences) {
        interfaceLocale = preferences.interfaceLanguage.rawValue
        internalContextLocale = preferences.internalContextLanguage.rawValue
    }

    var domainModel: UserLanguagePreferences {
        UserLanguagePreferences(
            interfaceLanguage: ChatOSLanguage(normalizing: interfaceLocale),
            internalContextLanguage: ChatOSLanguage(normalizing: internalContextLocale)
        )
    }
}
