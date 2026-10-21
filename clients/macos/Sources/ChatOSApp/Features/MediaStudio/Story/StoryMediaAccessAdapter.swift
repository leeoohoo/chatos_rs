import ChatOSCore
import Foundation

struct StoryMediaAccessAdapter: StoryMediaAccessResolving {
    let store: StoryProjectStore

    func resolveStoryMediaURL(
        ownerUserID: String,
        projectID: UUID,
        filename: String
    ) throws -> URL {
        try store.fileURL(
            filename,
            projectID: projectID,
            owner: ownerUserID
        )
    }
}
