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
    private var schedulerTask: Task<Void, Never>?
    private var schedulerNeedsAnotherPass = false
    private var communicationSchedulerTask: Task<Void, Never>?
    private var pendingCommunicationRoomIDs: Set<String> = []

    init(
        ownerUserID: String,
        service: NativeAgentGroupChatService,
        scheduler: LocalAgentGroupChatScheduler
    ) {
        self.ownerUserID = ownerUserID
        self.service = service
        self.scheduler = scheduler
    }

    deinit {
        observationTask?.cancel()
        schedulerTask?.cancel()
        communicationSchedulerTask?.cancel()
    }

    var creatorNamesByID: [String: String] {
        Dictionary(uniqueKeysWithValues: agents.map { ($0.id, $0.draft.name) })
    }

    func surveys(projectID: String) -> [LocalAgentRequirementSurvey] {
        surveys.filter { $0.projectID == projectID }
    }

    func activate(projectIDs: [String]) async {
        observedProjectIDs = projectIDs
        await load(projectIDs: projectIDs)
        startScheduler()
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
            let room = try await store.activeRoom(
                ownerUserID: ownerUserID,
                projectID: survey.projectID
            )
            if let room {
                await service.publishChange(.init(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    kind: .roomUpdated
                ))
            }
            await load(projectIDs: observedProjectIDs)
            if let room {
                startCommunicationScheduler(roomID: room.id)
            }
            startScheduler()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    private func startScheduler() {
        schedulerNeedsAnotherPass = true
        guard schedulerTask == nil else { return }
        schedulerTask = Task { [weak self] in
            guard let self else { return }
            repeat {
                schedulerNeedsAnotherPass = false
                do {
                    _ = try await scheduler.drainAccount(ownerUserID: ownerUserID)
                } catch is CancellationError {
                    // Another visible Agent surface may already own the
                    // account-wide drain lease. Its durable updates are still
                    // observed by this page.
                } catch {
                    errorMessage = "答案已保存，但 Agent 调度失败：\(error.localizedDescription)"
                }
                await load(projectIDs: observedProjectIDs)
            } while schedulerNeedsAnotherPass && !Task.isCancelled
            schedulerTask = nil
        }
    }

    private func startCommunicationScheduler(roomID: String) {
        pendingCommunicationRoomIDs.insert(roomID)
        guard communicationSchedulerTask == nil else { return }
        communicationSchedulerTask = Task { [weak self] in
            guard let self else { return }
            while !pendingCommunicationRoomIDs.isEmpty, !Task.isCancelled {
                guard let roomID = pendingCommunicationRoomIDs.sorted().first else { break }
                pendingCommunicationRoomIDs.remove(roomID)
                do {
                    _ = try await scheduler.drainCommunication(
                        ownerUserID: ownerUserID,
                        roomID: roomID
                    )
                } catch is CancellationError {
                    break
                } catch {
                    errorMessage = "答案已保存，但 Agent 沟通调度失败：\(error.localizedDescription)"
                }
                await load(projectIDs: observedProjectIDs)
            }
            communicationSchedulerTask = nil
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
            ZStack {
                LinearGradient(
                    colors: [
                        Color(nsColor: .windowBackgroundColor),
                        AppPalette.ai.opacity(0.045),
                    ],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
                .ignoresSafeArea()

                ScrollView {
                    VStack(alignment: .leading, spacing: 22) {
                        overviewHeader

                        if projects.isEmpty {
                            ContentUnavailableView {
                                Label("暂无项目", systemImage: "folder")
                            } description: {
                                Text("创建项目后，可以在这里查看该项目的需求调研。")
                            }
                            .frame(maxWidth: .infinity, minHeight: 360)
                            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18))
                        } else {
                            HStack(alignment: .firstTextBaseline) {
                                Text("项目")
                                    .appFont(.title2.weight(.semibold))
                                Text("选择项目查看历次调研")
                                    .appFont(.caption)
                                    .foregroundStyle(.secondary)
                                Spacer()
                            }

                            LazyVGrid(
                                columns: [GridItem(.adaptive(minimum: 310, maximum: 480), spacing: 16)],
                                spacing: 16
                            ) {
                                ForEach(projects) { project in
                                    NavigationLink(value: project.id) {
                                        projectCard(project)
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                        }
                    }
                    .padding(26)
                    .frame(maxWidth: 1480)
                    .frame(maxWidth: .infinity)
                }
                .navigationDestination(for: String.self) { projectID in
                    if let project = projects.first(where: { $0.id == projectID }) {
                        ProjectRequirementSurveysView(
                            surveys: viewModel.surveys(projectID: projectID),
                            submittingSurveyIDs: viewModel.submittingSurveyIDs,
                            creatorNamesByID: viewModel.creatorNamesByID,
                            showsNavigationBackButton: true,
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

    private var overviewHeader: some View {
        let pending = viewModel.surveys.filter { $0.status == .pending }.count
        let waiting = viewModel.surveys.filter {
            $0.status == .submitted && $0.resolution == nil
        }.count
        let resolved = viewModel.surveys.filter { $0.resolution != nil }.count

        return HStack(spacing: 20) {
            ZStack {
                RoundedRectangle(cornerRadius: 17, style: .continuous)
                    .fill(
                        LinearGradient(
                            colors: [AppPalette.ai, Color.indigo],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        )
                    )
                Image(systemName: "list.clipboard.fill")
                    .font(.system(size: 28, weight: .semibold))
                    .foregroundStyle(.white)
            }
            .frame(width: 64, height: 64)
            .shadow(color: AppPalette.ai.opacity(0.22), radius: 12, y: 5)

            VStack(alignment: .leading, spacing: 6) {
                Text("需求调研")
                    .appFont(.largeTitle.weight(.bold))
                Text("把关键选择沉淀为方案、执行计划和可追踪的项目决定")
                    .appFont(.body)
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 20)

            surveyMetric(value: pending, title: "待填写", color: AppPalette.ai)
            surveyMetric(value: waiting, title: "待方案", color: .orange)
            surveyMetric(value: resolved, title: "已完成", color: .green)
        }
        .padding(24)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(AppPalette.ai.opacity(0.13), lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.035), radius: 14, y: 5)
    }

    private func surveyMetric(value: Int, title: String, color: Color) -> some View {
        VStack(alignment: .trailing, spacing: 3) {
            Text("\(value)")
                .appFont(.title2.weight(.bold))
                .monospacedDigit()
                .foregroundStyle(color)
            Text(title)
                .appFont(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(minWidth: 54, alignment: .trailing)
    }

    private func projectCard(_ project: ResourceItem) -> some View {
        let surveys = viewModel.surveys(projectID: project.id)
        let pending = surveys.filter { $0.status == .pending }.count
        let waiting = surveys.filter { $0.status == .submitted && $0.resolution == nil }.count
        let resolved = surveys.filter { $0.resolution != nil }.count
        let accent: Color = pending > 0 ? AppPalette.ai : (waiting > 0 ? .orange : .green)

        return VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .top, spacing: 12) {
                ZStack {
                    RoundedRectangle(cornerRadius: 11, style: .continuous)
                        .fill(accent.opacity(0.11))
                    Image(systemName: "folder.fill")
                        .font(.system(size: 18, weight: .semibold))
                        .foregroundStyle(accent)
                }
                .frame(width: 42, height: 42)

                VStack(alignment: .leading, spacing: 4) {
                    Text(project.title)
                        .appFont(.headline)
                        .lineLimit(1)
                    Text(surveys.isEmpty ? "等待首次调研" : "已沉淀 \(surveys.count) 次需求确认")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }

                Spacer(minLength: 8)
                Image(systemName: "chevron.right")
                    .appFont(.caption.weight(.bold))
                    .foregroundStyle(.tertiary)
                    .padding(.top, 12)
            }

            HStack(spacing: 8) {
                projectStatusBadge("\(pending) 待填写", color: AppPalette.ai, isActive: pending > 0)
                projectStatusBadge("\(waiting) 待方案", color: .orange, isActive: waiting > 0)
                projectStatusBadge("\(resolved) 已完成", color: .green, isActive: resolved > 0)
            }

            if !surveys.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Text("处理进度")
                            .appFont(.caption2)
                            .foregroundStyle(.secondary)
                        Spacer()
                        Text("\(resolved) / \(surveys.count)")
                            .appFont(.caption2.weight(.semibold))
                            .monospacedDigit()
                    }
                    ProgressView(value: Double(resolved), total: Double(surveys.count))
                        .tint(accent)
                }
            }

            if viewModel.isLoading && surveys.isEmpty {
                ProgressView()
                    .controlSize(.small)
            }
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .stroke(accent.opacity(0.14), lineWidth: 1)
        }
        .shadow(color: .black.opacity(0.035), radius: 10, y: 4)
        .contentShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private func projectStatusBadge(
        _ title: String,
        color: Color,
        isActive: Bool
    ) -> some View {
        Text(title)
            .appFont(.caption2.weight(.medium))
            .foregroundStyle(isActive ? color : .secondary)
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background(
                (isActive ? color.opacity(0.1) : AppPalette.inputSurface.opacity(0.7)),
                in: Capsule()
            )
    }
}
