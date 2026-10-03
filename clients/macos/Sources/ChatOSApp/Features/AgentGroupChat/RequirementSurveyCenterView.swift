import ChatOSConnector
import SwiftUI

enum RequirementSurveyMonitoringPolicy {
    static let eventDebounce = Duration.milliseconds(250)
    static let consistencyCheckInterval = Duration.seconds(300)
    private static let refreshEventTypes: Set<String> = [
        "user_input_requested",
        "requirement_survey_resolved",
        "run_cancelled",
    ]

    static func shouldRefresh(for events: [LocalAgentEventRecord]) -> Bool {
        shouldRefresh(forEventTypes: events.map(\.eventType))
    }

    static func shouldRefresh(forEventTypes eventTypes: [String]) -> Bool {
        eventTypes.contains(where: refreshEventTypes.contains)
    }
}

@MainActor
private final class RequirementSurveyCenterViewModel: ObservableObject {
    @Published var surveys: [LocalAgentHostRequirementSurvey] = []
    @Published var submittingSurveyIDs: Set<String> = []
    @Published var isLoading = false
    @Published var errorMessage: String?

    private let ownerUserID: String
    private let client: NativeLocalAgentRequirementSurveyClient
    private let eventHub: NativeLocalAgentEventHub
    private var eventTask: Task<Void, Never>?
    private var consistencyTask: Task<Void, Never>?
    private var debouncedRefreshTask: Task<Void, Never>?
    private var isRefreshInFlight = false
    private var needsRefresh = false

    init(
        ownerUserID: String,
        client: NativeLocalAgentRequirementSurveyClient,
        eventHub: NativeLocalAgentEventHub
    ) {
        self.ownerUserID = ownerUserID
        self.client = client
        self.eventHub = eventHub
    }

    deinit {
        eventTask?.cancel()
        consistencyTask?.cancel()
        debouncedRefreshTask?.cancel()
    }

    func surveys(projectID: String) -> [LocalAgentHostRequirementSurvey] {
        surveys.filter { $0.projectResourceID == projectID }
    }

    func activate() async {
        if eventTask == nil {
            // Subscribe before the first list request so a survey created during activation
            // cannot be missed until the low-frequency consistency pass.
            let updates = await eventHub.updates()
            guard !Task.isCancelled else { return }
            eventTask = Task { [weak self, ownerUserID] in
                for await update in updates {
                    guard !Task.isCancelled, let self else { return }
                    guard update.ownerUserID == ownerUserID else { continue }
                    switch update.kind {
                    case .reconcile:
                        await self.load(showsProgress: false)
                    case let .events(events):
                        guard RequirementSurveyMonitoringPolicy.shouldRefresh(for: events) else {
                            continue
                        }
                        self.scheduleEventRefresh()
                    }
                }
            }
        }
        if consistencyTask == nil {
            consistencyTask = Task { [weak self] in
                while !Task.isCancelled {
                    do {
                        try await Task.sleep(
                            for: RequirementSurveyMonitoringPolicy.consistencyCheckInterval
                        )
                    } catch {
                        return
                    }
                    guard !Task.isCancelled, let self else { return }
                    await self.load(showsProgress: false)
                }
            }
        }
        await load()
    }

    private func scheduleEventRefresh() {
        debouncedRefreshTask?.cancel()
        debouncedRefreshTask = Task { [weak self] in
            do {
                try await Task.sleep(for: RequirementSurveyMonitoringPolicy.eventDebounce)
            } catch {
                return
            }
            guard !Task.isCancelled, let self else { return }
            await self.load(showsProgress: false)
        }
    }

    func load(showsProgress: Bool = true) async {
        guard !isRefreshInFlight else {
            needsRefresh = true
            return
        }
        isRefreshInFlight = true
        if showsProgress { isLoading = true }
        defer {
            isRefreshInFlight = false
            if showsProgress { isLoading = false }
        }
        repeat {
            needsRefresh = false
            do {
                surveys = try await client.list(ownerUserID: ownerUserID).sorted {
                    ($0.createdAtUnixMs, $0.id) > ($1.createdAtUnixMs, $1.id)
                }
                errorMessage = nil
            } catch {
                errorMessage = error.localizedDescription
            }
        } while needsRefresh && !Task.isCancelled
    }

    func submit(
        survey: LocalAgentHostRequirementSurvey,
        answers: [String: LocalAgentJSONValue]
    ) async -> Bool {
        guard submittingSurveyIDs.insert(survey.id).inserted else { return false }
        defer { submittingSurveyIDs.remove(survey.id) }
        do {
            let resolution = try await client.resolve(
                ownerUserID: ownerUserID,
                surveyID: survey.id,
                expectedVersion: survey.version,
                answers: answers
            )
            if let index = surveys.firstIndex(where: { $0.id == survey.id }) {
                surveys[index] = resolution.survey
            }
            errorMessage = nil
            return true
        } catch {
            errorMessage = error.localizedDescription
            await load(showsProgress: false)
            return false
        }
    }
}

struct RequirementSurveyCenterView: View {
    let projects: [ResourceItem]
    @StateObject private var viewModel: RequirementSurveyCenterViewModel
    @State private var projectPage = 0
    @State private var projectPageSize = 20

    init(
        ownerUserID: String,
        projects: [ResourceItem],
        client: NativeLocalAgentRequirementSurveyClient,
        eventHub: NativeLocalAgentEventHub
    ) {
        self.projects = projects
        _viewModel = StateObject(wrappedValue: RequirementSurveyCenterViewModel(
            ownerUserID: ownerUserID,
            client: client,
            eventHub: eventHub
        ))
    }

    var body: some View {
        NavigationStack {
            ZStack {
                LinearGradient(
                    colors: [Color(nsColor: .windowBackgroundColor), AppPalette.ai.opacity(0.045)],
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
                                Text("创建项目后，可以在这里查看本地任务发起的需求调研。")
                            }
                            .frame(maxWidth: .infinity, minHeight: 360)
                            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18))
                        } else {
                            HStack(alignment: .firstTextBaseline) {
                                Text("项目").appFont(.title2.weight(.semibold))
                                Text("选择项目查看本地任务的调研记录")
                                    .appFont(.caption)
                                    .foregroundStyle(.secondary)
                                Spacer()
                            }

                            LazyVGrid(
                                columns: [GridItem(.adaptive(minimum: 310, maximum: 480), spacing: 16)],
                                spacing: 16
                            ) {
                                ForEach(projects.agentPage(index: projectPage, size: projectPageSize)) { project in
                                    NavigationLink(value: project.id) { projectCard(project) }
                                        .buttonStyle(.plain)
                                }
                            }
                            AgentListPaginationBar(
                                totalCount: projects.count,
                                page: $projectPage,
                                pageSize: $projectPageSize
                            )
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
                            showsNavigationBackButton: true,
                            heading: project.title,
                            explanation: "本地任务需要你补充信息时会在这里暂停。提交答案后，Local Agent Host 会原子保存答案并恢复原 Run。",
                            onSubmit: viewModel.submit
                        )
                        .navigationTitle(project.title)
                    }
                }
            }
            .navigationTitle("需求调研")
            .toolbar {
                Button { Task { await viewModel.load() } } label: {
                    Label("刷新", systemImage: "arrow.clockwise")
                }
                .disabled(viewModel.isLoading)
            }
        }
        .task { await viewModel.activate() }
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
        let open = viewModel.surveys.filter { $0.status == .open }.count
        let resolved = viewModel.surveys.filter { $0.status == .resolved }.count
        return HStack(spacing: 20) {
            ZStack {
                RoundedRectangle(cornerRadius: 17, style: .continuous)
                    .fill(LinearGradient(
                        colors: [AppPalette.ai, Color.indigo],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    ))
                Image(systemName: "list.clipboard.fill")
                    .font(.system(size: 28, weight: .semibold))
                    .foregroundStyle(.white)
            }
            .frame(width: 64, height: 64)
            .shadow(color: AppPalette.ai.opacity(0.22), radius: 12, y: 5)

            VStack(alignment: .leading, spacing: 6) {
                Text("需求调研").appFont(.largeTitle.weight(.bold))
                Text("由 Local Agent Host 持久化；答案提交后直接恢复对应的本地任务")
                    .appFont(.body)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 20)
            surveyMetric(value: open, title: "待填写", color: AppPalette.ai)
            surveyMetric(value: resolved, title: "已恢复", color: .green)
        }
        .padding(24)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay { RoundedRectangle(cornerRadius: 18).stroke(AppPalette.ai.opacity(0.13)) }
        .shadow(color: .black.opacity(0.035), radius: 14, y: 5)
    }

    private func surveyMetric(value: Int, title: String, color: Color) -> some View {
        VStack(alignment: .trailing, spacing: 3) {
            Text("\(value)")
                .appFont(.title2.weight(.bold))
                .monospacedDigit()
                .foregroundStyle(color)
            Text(title).appFont(.caption).foregroundStyle(.secondary)
        }
        .frame(minWidth: 54, alignment: .trailing)
    }

    private func projectCard(_ project: ResourceItem) -> some View {
        let surveys = viewModel.surveys(projectID: project.id)
        let open = surveys.filter { $0.status == .open }.count
        let resolved = surveys.filter { $0.status == .resolved }.count
        let accent: Color = open > 0 ? AppPalette.ai : .green
        return VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .top, spacing: 12) {
                ZStack {
                    RoundedRectangle(cornerRadius: 11).fill(accent.opacity(0.11))
                    Image(systemName: "folder.fill").foregroundStyle(accent)
                }
                .frame(width: 42, height: 42)
                VStack(alignment: .leading, spacing: 4) {
                    Text(project.title).appFont(.headline).lineLimit(1)
                    Text(surveys.isEmpty ? "暂无本地任务调研" : "共 \(surveys.count) 张调研单")
                        .appFont(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Image(systemName: "chevron.right").foregroundStyle(.tertiary).padding(.top, 12)
            }
            HStack(spacing: 8) {
                statusBadge("\(open) 待填写", color: AppPalette.ai, isActive: open > 0)
                statusBadge("\(resolved) 已恢复", color: .green, isActive: resolved > 0)
            }
            if viewModel.isLoading && surveys.isEmpty { ProgressView().controlSize(.small) }
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
        .overlay { RoundedRectangle(cornerRadius: 16).stroke(accent.opacity(0.14)) }
        .shadow(color: .black.opacity(0.035), radius: 10, y: 4)
    }

    private func statusBadge(_ title: String, color: Color, isActive: Bool) -> some View {
        Text(title)
            .appFont(.caption2.weight(.medium))
            .foregroundStyle(isActive ? color : .secondary)
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background(isActive ? color.opacity(0.1) : AppPalette.inputSurface.opacity(0.7), in: Capsule())
    }
}
