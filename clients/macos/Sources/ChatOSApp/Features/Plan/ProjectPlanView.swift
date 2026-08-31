import ChatOSCore
import SwiftUI

struct ProjectPlanView: View {
    @StateObject private var viewModel: ProjectPlanViewModel
    private let graphService: any MessageTaskGraphServicing
    private let executionService: any ProjectExecutionServicing
    private let realtimeService: any ConversationRealtimeStreaming
    @State private var presentation: ProjectPlanPresentation?

    init(
        projectID: String,
        service: any ProjectPlanServicing,
        graphService: any MessageTaskGraphServicing,
        executionService: any ProjectExecutionServicing,
        realtimeService: any ConversationRealtimeStreaming
    ) {
        _viewModel = StateObject(wrappedValue: ProjectPlanViewModel(projectID: projectID, service: service))
        self.graphService = graphService
        self.executionService = executionService
        self.realtimeService = realtimeService
    }

    var body: some View {
        VStack(spacing: 0) {
            ProjectPlanHeaderView(viewModel: viewModel)

            if let errorMessage = viewModel.errorMessage {
                ProjectPlanErrorBanner(
                    message: errorMessage,
                    onRetry: { Task { await viewModel.load() } },
                    onDismiss: viewModel.dismissError
                )
            }

            Group {
                if viewModel.isLoading && viewModel.snapshot == nil {
                    ProgressView("正在加载 Plan…")
                        .workspaceFill()
                } else if viewModel.requirements.isEmpty {
                    ContentUnavailableView(
                        "暂无需求",
                        systemImage: "list.bullet.clipboard",
                        description: Text("完成规划后，需求、技术文档和项目任务会显示在这里。")
                    )
                    .workspaceFill()
                } else {
                    HSplitView {
                        RequirementBrowserView(viewModel: viewModel)
                            .frame(width: browserWidth)
                        RequirementDetailView(
                            viewModel: viewModel,
                            onPreviewScope: { presentation = .scope($0) },
                            onOpenExecution: openExecution,
                            onStartExecution: { presentation = .start($0) }
                        )
                        .frame(minWidth: 620)
                    }
                    .workspaceFill()
                }
            }
        }
        .workspaceFill()
        .task { await viewModel.load() }
        .onChange(of: viewModel.selectedRequirementID) { oldValue, newValue in
            guard oldValue != nil, oldValue != newValue else { return }
            Task { await viewModel.loadSelection() }
        }
        .sheet(item: $presentation) { item in
            Group {
                switch item {
                case let .scope(requirement):
                    RequirementScopePreviewSheet(
                        requirement: requirement,
                        viewModel: viewModel
                    )
                case let .start(requirement):
                    RequirementExecutionStartSheet(
                        requirement: requirement,
                        isStarting: viewModel.isCreatingExecution,
                        onStart: { feedback in
                            Task {
                                guard let launch = await viewModel.createExecution(planningFeedback: feedback) else { return }
                                presentation = .workspace(viewModel.makeExecutionTurn(requirement: requirement, launch: launch))
                            }
                        }
                    )
                case let .workspace(turn):
                    MessageTaskWorkspaceSheet(
                        turn: turn,
                        graphService: graphService,
                        projectExecutionService: executionService,
                        realtimeService: realtimeService
                    )
                }
            }
            .id(item.id)
        }
    }

    private var browserWidth: CGFloat {
        min(CGFloat(max(viewModel.requirementColumns.count, 1)) * 320, 860)
    }

    private func openExecution(_ requirement: ProjectRequirement) {
        guard let launch = viewModel.execution else {
            presentation = .start(requirement)
            return
        }
        presentation = .workspace(viewModel.makeExecutionTurn(requirement: requirement, launch: launch))
    }

}

private enum ProjectPlanPresentation: Identifiable {
    case scope(ProjectRequirement)
    case start(ProjectRequirement)
    case workspace(ConversationTurn)

    var id: String {
        switch self {
        case let .scope(requirement): "scope-\(requirement.id)"
        case let .start(requirement): "start-\(requirement.id)"
        case let .workspace(turn): "workspace-\(turn.id)"
        }
    }
}
