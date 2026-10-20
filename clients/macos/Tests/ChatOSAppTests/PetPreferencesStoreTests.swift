@testable import ChatOSApp
import Foundation
import XCTest

@MainActor
final class PetPreferencesStoreTests: XCTestCase {
    func testFavoriteProjectPersistsLocallyAndNormalizesIdentifier() throws {
        let suiteName = "PetPreferencesStoreTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = PetPreferencesStore(defaults: defaults)

        store.setFavorite(true, projectID: " project-b ")
        store.setFavorite(true, projectID: "project-a")

        XCTAssertEqual(store.favoriteProjectIDs, ["project-a", "project-b"])
        XCTAssertTrue(store.isFavorite(projectID: "project-b"))
        let restored = PetPreferencesStore(defaults: defaults)
        XCTAssertEqual(restored.favoriteProjectIDs, ["project-a", "project-b"])
    }

    func testRemovingFavoritePersistsWithoutDeletingAnyProjectData() throws {
        let suiteName = "PetPreferencesStoreTests.\(UUID().uuidString)"
        let defaults = try XCTUnwrap(UserDefaults(suiteName: suiteName))
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let store = PetPreferencesStore(defaults: defaults)
        store.setFavorite(true, projectID: "project")

        store.setFavorite(false, projectID: "project")

        XCTAssertFalse(store.isFavorite(projectID: "project"))
        XCTAssertTrue(PetPreferencesStore(defaults: defaults).favoriteProjectIDs.isEmpty)
    }
}
