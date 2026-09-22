import ChatOSConnector
import ChatOSCore
import SwiftUI

@MainActor
private final class RequirementSurveyCenterViewModel: ObservableObject {
    @Published var surveys: [LocalAgentRequirementSurvey] = []
    @Published var agents: [LocalAgentProfile] = []
    @Published var submittingSurveyIDs: Set<String> = []
    @Published var isLoading = false
    @Published var errorMessage: String?

    let ownerUserID: String
    let service: NativeAgentGroupChatService
    let scheduler: LocalAgentGroupChatScheduler
    private var observedProjectIDs: [String] = []
    private var observationTask: Task<Void, Never>?

    init(
        ownerUserID: String,
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler
    ) {
        self.ownerUserID = ownerUserID
        self.service = service
        self.scheduler = scheduler
    }

    deinit { observationTask?.cancel() }

    var creatorNamesByID: [String: String] {
        Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0.draft.name) })
    }

    func surveys(projectID: String) -> [LocalAgentRequirementSurvey] {
        surveys.filter { $0.projectID == projectID }
    }

    func activate(projectIDs: [String]) async {
        observedProjectIDs = projectIDs
        await load(projectIDs: projectIDs)
        guard observationTask == nil else { return }
        observationTask = Task { [weak self] in
            guard let self else { return }
            let changes = await service.changes(ownerUserID: ownerUserID)
            for await _ in changes {
                guard !Task.isCancelled else { break }
                await load(projectIDs: observedProjectIDs)
            }
        }
    }

    func load(projectIDs: [String]) async {
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let store = try await service.store()
            let agents = try await store.listAgents(
                ownerUserID: ownerUserID,
                includeArchived: false
            )
            var surveys: [LocalAgentRequirementSurvey] = []
            for projectID in projectIDs {
                surveys += try await store.listRequirementSurveys(
                    ownerUserID: ownerUserID,
                    projectID: projectID,
                    status: nil
                )
            }
            self.agents = agents
            self.surveys = surveys.sorted {
                ($0.createdAtUnixMs, $0.id) > ($1.createdAtUnixMs, $1.id)
            }
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func submit(
        survey: LocalAgentRequirementSurvey,
        selections: [String: Set<String>],
        notes: String
    ) async -> Bool {
        guard submittingSurveyIDs.insert(survey.id).inserted else { return false }
        defer { submittingSurveyIDs.remove(survey.id) }
        do {
            let answers = survey.draft.questions.compactMap { question -> LocalAgentRequirementSurveyAnswer? in
                let selected = selections[question.id] ?? []
                guard !selected.isEmpty else { return nil }
                return .init(
                    questionID: question.id,
                    selectedOptionIDs: question.options.map(\.id).filter(selected.contains)
                )
            }
            let store = try await service.store()
            _ = try await store.submitRequirementSurvey(
                ownerUserID: ownerUserID,
                projectID: survey.projectID,
                surveyID: survey.id,
                submission: .init(
                    answers: answers,
                    notes: notes.trimmingCharacters(in: .whitespacesAndNewlines)
                ),
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            if let room = try await store.activeRoom(
                ownerUserID: ownerUserID,
                projectID: survey.projectID
            ) {
                await service.publishChange(.init(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    kind: .roomUpdated
                ))
            }
            _ = try await scheduler.drainAccount(ownerUserID: ownerUserID)
            await load(projectIDs: observedProjectIDs)
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }
}

struct RequirementSurveyCenterView: View {
    let projects: [ResourceItem]
    @StateObject private var viewModel: RequirementSurveyCenterViewModel

    init(
        ownerUserID: String,
        projects: [ResourceItem],
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler
    ) {
        self.projects = projects
        _viewModel = StateObject(wrappedValue: RequirementSurveyCenterViewModel(
            ownerUserID: ownerUserID,
            service: service,
            scheduler: scheduler
        ))
    }

    var body: some View {
        NavigationStack {
            Group {
                if projects.isEmpty {
                    ContentUnavailableView {
                        Label("暂无项目", systemImage: "folder")
                    } description: {
                        Text("创建项目后，可以在这里查看该项目的需求调研。")
                    }
                } else {
                    List(projects) { project in
                        NavigationLink(value: project.id) {
                            projectRow(project)
                        }
                    }
                    .listStyle(.inset)
                    .navigationDestination(for: String.self) { projectID in
                        if let project = projects.first(where: { $0.id == projectID }) {
                            ProjectRequirementSurveysView(
                                surveys: viewModel.surveys(projectID: projectID),
                                submittingSurveyIDs: viewModel.submittingSurveyIDs,
                                creatorNamesByID: viewModel.creatorNamesByID,
                                heading: project.title,
                                explanation: "本项目的新需求、重大变更、方案取舍和验收确认。提交答案后，负责 Agent 会在原调研单下形成解决方案、执行计划、风险与相关资料。",
                                onSubmit: { survey, selections, notes in
                                    await viewModel.submit(
                                        survey: survey,
                                        selections: selections,
                                        notes: notes
                                    )
                                }
                            )
                            .navigationTitle(project.title)
                        }
                    }
                }
            }
            .navigationTitle("需求调研")
        }
        .task(id: projects.map(\.id).joined(separator: "|")) {
            await viewModel.activate(projectIDs: projects.map(\.id))
        }
        .alert(
            "需求调研错误",
            isPresented: Binding(
                get: { viewModel.errorMessage != nil },
                set: { if !$0 { viewModel.errorMessage = nil } }
            )
        ) {
            Button("好", role: .cancel) { viewModel.errorMessage = nil }
        } message: {
            Text(viewModel.errorMessage ?? "")
        }
    }

    private func projectRow(_ project: ResourceItem) -> some View {
        let surveys = viewModel.surveys(projectID: project.id)
        let pending = surveys.filter { $0.status == .pending }.count
        let waiting = surveys.filter { $0.status == .submitted && $0.resolution == nil }.count
        return HStack(spacing: 12) {
            Image(systemName: "folder.fill")
                .font(.title3)
                .foregroundStyle(.orange)
                .frame(width: 28)
            Text(project.title)
                .appFont(.body)
                .fontWeight(.medium)
            Spacer()
            if viewModel.isLoading && surveys.isEmpty {
                ProgressView().controlSize(.small)
            } else {
                Text(pending > 0 ? "\(pending) 待填写" : waiting > 0 ? "\(waiting) 待方案" : "\(surveys.count) 份调研")
                    .appFont(.caption)
                    .foregroundStyle(pending > 0 ? AppPalette.ai : .secondary)
            }
        }
        .padding(.vertical, 6)
    }
}
