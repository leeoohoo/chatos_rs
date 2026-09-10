import Testing
@testable import ChatOSApp

struct ProjectPluginWorkspaceTests {
    @Test func workspaceDoesNotDuplicateTheApplicationsEntry() {
        #expect(ProjectWorkspaceTab.allCases == [.directory, .messages, .settings])
        #expect(ProjectWorkspaceTab.allCases.map { $0.title(language: .english) } == ["Project Files", "Messages", "Project Settings"])
    }
}
