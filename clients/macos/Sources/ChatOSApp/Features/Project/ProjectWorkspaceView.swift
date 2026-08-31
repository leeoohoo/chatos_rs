import SwiftUI

struct ProjectWorkspaceView: View {
    @EnvironmentObject private var model: AppModel
    let projectID: String

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Picker(
                    model.localized("项目工作面", english: "Project Workspace"),
                    selection: $model.projectTab
                ) {
                    ForEach(ProjectWorkspaceTab.allCases) { tab in
                        Text(tab.title(language: model.interfaceLanguage)).tag(tab)
                    }
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(maxWidth: 460)
                Spacer()
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 10)
            .background(.bar)

            Divider()
            selectedWorkspace
                .workspaceFill()
        }
        .workspaceFill()
        .navigationTitle(
            model.projects.first(where: { $0.id == projectID })?.title ?? projectID
        )
    }

    @ViewBuilder
    private var selectedWorkspace: some View {
        Group {
            let project = model.workspaceProject(id: projectID)
            switch model.projectTab {
            case .directory:
                ProjectDirectoryView(
                    projectID: projectID,
                    rootPath: project?.rootPath,
                    service: model.projectFilesystemService,
                    codeNavigationService: model.projectCodeNavigationService,
                    gitService: model.projectGitService
                )
                .id(project?.rootPath ?? projectID)
            case .messages:
                ProjectMessagesView(projectID: projectID)
            case .plan:
                ProjectPlanView(
                    projectID: projectID,
                    service: model.projectPlanService,
                    graphService: model.messageTaskGraphService,
                    executionService: model.projectExecutionService,
                    realtimeService: model.realtimeService
                )
            case .settings:
                ProjectRunSettingsView(
                    projectID: projectID,
                    projectName: project?.name ?? projectID,
                    rootPath: project?.displayRootPath ?? project?.rootPath,
                    service: model.projectRunService,
                    petPreferences: model.petPreferences
                )
            }
        }
        .workspaceFill()
    }
}
