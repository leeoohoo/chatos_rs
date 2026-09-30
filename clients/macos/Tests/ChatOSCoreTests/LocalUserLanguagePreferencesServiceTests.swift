import ChatOSCore
import Foundation
import XCTest

final class LocalUserLanguagePreferencesServiceTests: XCTestCase {
    func testFetchReturnsLocalDefaultsForNewUser() async throws {
        let (service, suiteName) = makeService()
        defer { UserDefaults.standard.removePersistentDomain(forName: suiteName) }

        let preferences = try await service.fetch(userID: "user-new")

        XCTAssertEqual(preferences.interfaceLanguage, .simplifiedChinese)
        XCTAssertEqual(preferences.internalContextLanguage, .simplifiedChinese)
    }

    func testUpdatePersistsBothLanguagesForTheSameUser() async throws {
        let (service, suiteName) = makeService()
        defer { UserDefaults.standard.removePersistentDomain(forName: suiteName) }

        let saved = try await service.update(
            userID: "user-1",
            preferences: .init(
                interfaceLanguage: .simplifiedChinese,
                internalContextLanguage: .english
            )
        )
        let fetched = try await service.fetch(userID: "user-1")

        XCTAssertEqual(saved.interfaceLanguage, .simplifiedChinese)
        XCTAssertEqual(saved.internalContextLanguage, .english)
        XCTAssertEqual(fetched, saved)
    }

    func testPreferencesAreIsolatedByUser() async throws {
        let (service, suiteName) = makeService()
        defer { UserDefaults.standard.removePersistentDomain(forName: suiteName) }

        _ = try await service.update(
            userID: "user-a",
            preferences: .init(
                interfaceLanguage: .english,
                internalContextLanguage: .english
            )
        )

        let otherUser = try await service.fetch(userID: "user-b")

        XCTAssertEqual(otherUser.interfaceLanguage, .simplifiedChinese)
        XCTAssertEqual(otherUser.internalContextLanguage, .simplifiedChinese)
    }

    private func makeService() -> (LocalUserLanguagePreferencesService, String) {
        let suiteName = "LocalUserLanguagePreferencesServiceTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        return (LocalUserLanguagePreferencesService(userDefaults: defaults), suiteName)
    }
}
