import Foundation

/// Narrow local-preference port used by project features. Favorites are a
/// device-wide presentation preference in the current macOS product; callers
/// still intersect the stored IDs with the current account's project list.
@MainActor
public protocol ProjectFavoritePreferences: AnyObject {
    var favoriteProjectIDs: Set<String> { get }
    func setFavorite(_ isFavorite: Bool, projectID: String)
}
