import Foundation

@MainActor
public struct ProjectFavoriteUseCase {
    private let preferences: any ProjectFavoritePreferences

    public init(preferences: any ProjectFavoritePreferences) {
        self.preferences = preferences
    }

    public func isFavorite(projectID: String) -> Bool {
        preferences.favoriteProjectIDs.contains(projectID)
    }

    @discardableResult
    public func setFavorite(_ isFavorite: Bool, projectID: String) -> Bool {
        preferences.setFavorite(isFavorite, projectID: projectID)
        return preferences.favoriteProjectIDs.contains(projectID)
    }
}
