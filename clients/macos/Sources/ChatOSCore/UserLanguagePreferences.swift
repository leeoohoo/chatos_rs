import Foundation

public enum ChatOSLanguage: String, Codable, CaseIterable, Identifiable, Sendable {
    case simplifiedChinese = "zh-CN"
    case english = "en-US"

    public var id: String { rawValue }

    public var locale: Locale {
        Locale(identifier: rawValue)
    }

    public init(normalizing value: String?) {
        self = value == Self.english.rawValue ? .english : .simplifiedChinese
    }
}

public struct UserLanguagePreferences: Equatable, Sendable {
    public var interfaceLanguage: ChatOSLanguage
    public var internalContextLanguage: ChatOSLanguage

    public init(
        interfaceLanguage: ChatOSLanguage,
        internalContextLanguage: ChatOSLanguage
    ) {
        self.interfaceLanguage = interfaceLanguage
        self.internalContextLanguage = internalContextLanguage
    }
}

public protocol UserLanguagePreferencesServicing: Sendable {
    func fetch() async throws -> UserLanguagePreferences
    func update(
        userID: String,
        preferences: UserLanguagePreferences
    ) async throws -> UserLanguagePreferences
}
