import Foundation

public actor LocalUserLanguagePreferencesService: UserLanguagePreferencesServicing {
    private let userDefaults: UserDefaults

    public init(userDefaults: UserDefaults = .standard) {
        self.userDefaults = userDefaults
    }

    public func fetch(userID: String) async throws -> UserLanguagePreferences {
        UserLanguagePreferences(
            interfaceLanguage: ChatOSLanguage(
                normalizing: userDefaults.string(forKey: key(userID: userID, field: "interfaceLanguage"))
            ),
            internalContextLanguage: ChatOSLanguage(
                normalizing: userDefaults.string(forKey: key(userID: userID, field: "internalContextLanguage"))
            )
        )
    }

    public func update(
        userID: String,
        preferences: UserLanguagePreferences
    ) async throws -> UserLanguagePreferences {
        userDefaults.set(
            preferences.interfaceLanguage.rawValue,
            forKey: key(userID: userID, field: "interfaceLanguage")
        )
        userDefaults.set(
            preferences.internalContextLanguage.rawValue,
            forKey: key(userID: userID, field: "internalContextLanguage")
        )
        return preferences
    }

    private func key(userID: String, field: String) -> String {
        "ChatOS.user.\(userID).\(field)"
    }
}
