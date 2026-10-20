import Testing
@testable import ChatOSCore

@MainActor
@Suite("ProjectFavoriteUseCase")
struct ProjectFavoriteUseCaseTests {
    @Test
    func readsAndMutatesThroughNarrowPreferencePort() {
        let preferences = ProjectFavoriteTestPreferences(ids: ["existing"])
        let useCase = ProjectFavoriteUseCase(preferences: preferences)

        #expect(useCase.isFavorite(projectID: "existing"))
        #expect(!useCase.isFavorite(projectID: "project"))
        #expect(useCase.setFavorite(true, projectID: "project"))
        #expect(preferences.favoriteProjectIDs == ["existing", "project"])
        #expect(!useCase.setFavorite(false, projectID: "project"))
        #expect(preferences.favoriteProjectIDs == ["existing"])
    }
}

@MainActor
private final class ProjectFavoriteTestPreferences: ProjectFavoritePreferences {
    private(set) var favoriteProjectIDs: Set<String>

    init(ids: Set<String>) {
        favoriteProjectIDs = ids
    }

    func setFavorite(_ isFavorite: Bool, projectID: String) {
        if isFavorite {
            favoriteProjectIDs.insert(projectID)
        } else {
            favoriteProjectIDs.remove(projectID)
        }
    }
}
