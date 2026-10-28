import ChatOSCore
import SwiftUI

struct StoryViewContext: Equatable {
    var interfaceLanguage: ChatOSLanguage
    var authenticatedUserID: String?

    func localized(_ chinese: String, english: String) -> String {
        interfaceLanguage == .english ? english : chinese
    }

    static let signedOut = StoryViewContext(
        interfaceLanguage: .simplifiedChinese,
        authenticatedUserID: nil
    )
}

private struct StoryViewContextKey: EnvironmentKey {
    static let defaultValue = StoryViewContext.signedOut
}

extension EnvironmentValues {
    var storyViewContext: StoryViewContext {
        get { self[StoryViewContextKey.self] }
        set { self[StoryViewContextKey.self] = newValue }
    }
}

struct StoryStudioScene: View, Equatable {
    let viewModel: StoryStudioViewModel
    let mediaStudio: MediaStudioViewModel
    let context: StoryViewContext

    nonisolated static func == (lhs: Self, rhs: Self) -> Bool {
        lhs.viewModel === rhs.viewModel
            && lhs.mediaStudio === rhs.mediaStudio
            && lhs.context == rhs.context
    }

    var body: some View {
        StoryStudioView(viewModel: viewModel, mediaStudio: mediaStudio)
            .environment(\.storyViewContext, context)
    }
}
