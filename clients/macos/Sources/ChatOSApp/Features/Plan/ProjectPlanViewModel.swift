import ChatOSCore
import Foundation

struct RequirementColumn: Identifiable, Equatable {
    var id: String
    var title: String
    var requirements: [ProjectRequirement]
}

@MainActor
final class ProjectPlanViewModel: ObservableObject {
    @Published private(set) var snapshot: ProjectPlanSnapshot?
    @Published private(set) var workItems: [ProjectWorkItem] = []
    @Published private(set) var edges: [ProjectPlanEdge] = []
    @Published private(set) var documents: [ProjectRequirementDocument] = []
    @Published private(set) var execution: ProjectRequirementExecutionLaunch?
    @Published private(set) var loadedWorkItemCounts: [String: Int] = [:]
    @Published var selectedRequirementID: String?
    @Published var selectedDocumentID: String?
    @Published var selectedSection: DetailSection = .requirement
    @Published private(set) var isLoading = false
    @Published private(set) var isLoadingSelection = false
    @Published private(set) var isCreatingExecution = false
    @Published private(set) var errorMessage: String?
    @Published var visibleWorkItemLimit = 80

    enum DetailSection: String, CaseIterable, Identifiable {
        case requirement = "需求"
        case documents = "技术文档"
        case tasks = "任务"
        var id: Self { self }

        func title(language: ChatOSLanguage) -> String {
            guard language == .english else { return rawValue }
            return switch self {
            case .requirement: "Requirement"
            case .documents: "Technical Documents"
            case .tasks: "Tasks"
            }
        }
    }

    let projectID: String
    private let service: any ProjectPlanServicing
    private var selectionGeneration: Int64 = 0

    init(projectID: String, service: any ProjectPlanServicing) {
        self.projectID = projectID
        self.service = service
    }

    var requirements: [ProjectRequirement] { snapshot?.requirements ?? [] }

    var selectedRequirement: ProjectRequirement? {
        requirements.first(where: { $0.id == selectedRequirementID })
    }

    var requirementByID: [String: ProjectRequirement] {
        Dictionary(uniqueKeysWithValues: requirements.map { ($0.id, $0) })
    }

    var requirementChildren: [String: [ProjectRequirement]] {
        let knownIDs = Set(requirements.map(\.id))
        return Dictionary(grouping: requirements.filter {
            guard let parentID = normalized($0.parentRequirementID) else { return false }
            return knownIDs.contains(parentID)
        }, by: { normalized($0.parentRequirementID)! })
    }

    var requirementPath: [String] {
        guard let selectedRequirementID else { return [] }
        let byID = requirementByID
        var result: [String] = []
        var visited = Set<String>()
        var current = byID[selectedRequirementID]
        while let requirement = current, visited.insert(requirement.id).inserted {
            result.insert(requirement.id, at: 0)
            current = normalized(requirement.parentRequirementID).flatMap { byID[$0] }
        }
        return result
    }

    var requirementColumns: [RequirementColumn] {
        let knownIDs = Set(requirements.map(\.id))
        let roots = requirements.filter {
            guard let parent = normalized($0.parentRequirementID) else { return true }
            return !knownIDs.contains(parent)
        }
        var columns = [RequirementColumn(id: "root", title: "主需求", requirements: roots)]
        let children = requirementChildren
        for requirementID in requirementPath {
            guard let values = children[requirementID], !values.isEmpty else { continue }
            columns.append(
                RequirementColumn(
                    id: "children-\(requirementID)",
                    title: requirementByID[requirementID]?.title ?? "子需求",
                    requirements: values
                )
            )
        }
        return columns
    }

    var selectedChildren: [ProjectRequirement] {
        selectedRequirementID.flatMap { requirementChildren[$0] } ?? []
    }

    var selectedPrerequisites: [ProjectRequirement] {
        relatedRequirements(from: requirementPrerequisiteIDs[selectedRequirementID ?? ""] ?? [])
    }

    var selectedDependents: [ProjectRequirement] {
        relatedRequirements(from: requirementDependentIDs[selectedRequirementID ?? ""] ?? [])
    }

    var selectedExecutionScope: [ProjectRequirement] {
        guard let selectedRequirementID else { return [] }
        return relatedRequirements(
            from: executionScopeIDs(includePrerequisiteDependents: false)
                .filter { $0 != selectedRequirementID }
        )
    }

    var requirementDependencyEdges: [ProjectPlanEdge] {
        let ids = Set(requirements.map(\.id))
        return (snapshot?.edges ?? []).filter {
            ids.contains($0.sourceID) && ids.contains($0.targetID)
                && $0.kind.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() != "contains"
        }
    }

    var selectedDocument: ProjectRequirementDocument? {
        documents.first(where: { $0.id == selectedDocumentID }) ?? documents.first
    }

    var sortedWorkItems: [ProjectWorkItem] {
        topologicallySorted(workItems)
    }

    var visibleWorkItems: [ProjectWorkItem] {
        Array(sortedWorkItems.prefix(visibleWorkItemLimit))
    }

    var hiddenWorkItemCount: Int {
        max(0, sortedWorkItems.count - visibleWorkItems.count)
    }

    var openWorkItemCount: Int {
        workItems.filter { !isDoneStatus($0.status) }.count
    }

    func load() async {
        isLoading = true
        errorMessage = nil
        do {
            let snapshot = try await service.fetchPlan(projectID: projectID)
            self.snapshot = snapshot
            loadedWorkItemCounts = [:]
            if selectedRequirementID == nil
                || !snapshot.requirements.contains(where: { $0.id == selectedRequirementID }) {
                selectedRequirementID = snapshot.requirements.first?.id
            }
            if selectedRequirementID != nil { await loadSelection() }
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }

    func loadSelection() async {
        guard let requirementID = selectedRequirementID else {
            workItems = []
            edges = []
            documents = []
            execution = nil
            return
        }
        selectionGeneration += 1
        let generation = selectionGeneration
        isLoadingSelection = true
        errorMessage = nil
        visibleWorkItemLimit = 80
        workItems = []
        edges = []
        documents = []
        selectedDocumentID = nil
        execution = nil

        let service = service
        let projectID = self.projectID
        async let detail = capturePlanLoad {
            try await service.fetchWorkItems(projectID: projectID, requirementID: requirementID)
        }
        async let docs = capturePlanLoad {
            try await service.fetchDocuments(projectID: projectID, requirementID: requirementID)
        }
        async let launch = capturePlanLoad {
            try await service.fetchExecution(projectID: projectID, requirementID: requirementID)
        }
        let values = await (detail, docs, launch)
        guard generation == selectionGeneration else { return }

        var errors: [String] = []
        switch values.0 {
        case let .success(detail):
            workItems = detail.workItems
            edges = detail.edges
            loadedWorkItemCounts[requirementID] = detail.workItems.count
        case let .failure(message):
            errors.append("项目任务：\(message)")
        }
        switch values.1 {
        case let .success(docs):
            documents = docs
            selectedDocumentID = docs.first?.id
        case let .failure(message):
            errors.append("技术文档：\(message)")
        }
        switch values.2 {
        case let .success(launch):
            execution = launch
        case let .failure(message):
            errors.append("执行计划：\(message)")
        }
        errorMessage = errors.isEmpty ? nil : errors.joined(separator: "；")
        if generation == selectionGeneration { isLoadingSelection = false }
    }

    func createExecution(planningFeedback: String?) async -> ProjectRequirementExecutionLaunch? {
        guard let requirementID = selectedRequirementID else { return nil }
        isCreatingExecution = true
        errorMessage = nil
        defer { isCreatingExecution = false }
        do {
            let launch = try await service.createExecution(
                projectID: projectID,
                requirementID: requirementID,
                includePrerequisiteDependents: false,
                planningFeedback: planningFeedback
            )
            execution = launch
            return launch
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func makeExecutionTurn(
        requirement: ProjectRequirement,
        launch: ProjectRequirementExecutionLaunch
    ) -> ConversationTurn {
        let messageID = normalized(launch.messageID) ?? launch.executionGroupID
        let createdAt = launch.createdAt ?? Date()
        return ConversationTurn(
            id: launch.executionGroupID,
            sessionID: launch.conversationID,
            sequence: 0,
            revision: 1,
            userMessage: ChatMessage(
                id: messageID,
                role: .user,
                text: "为需求“\(requirement.title)”生成执行计划",
                createdAt: createdAt
            ),
            messageTaskLookup: MessageTaskLookup(
                sessionID: launch.conversationID,
                turnID: launch.executionGroupID,
                sourceUserMessageID: messageID
            ),
            projectExecutionContext: ProjectExecutionContext(
                projectID: launch.projectID,
                requirementID: launch.requirementID,
                executionGroupID: launch.executionGroupID,
                contactID: launch.contactID,
                mode: "project_requirement_execution",
                executionKind: "project_requirement_execution",
                confirmationStatus: launch.confirmationStatus,
                overallStatus: launch.overallStatus
            ),
            status: launch.hasStartedRuns ? .streaming : .queued,
            startedAt: createdAt
        )
    }

    func loadMoreWorkItems() {
        visibleWorkItemLimit += 80
    }

    func loadedWorkItemCount(for requirementID: String) -> Int? {
        loadedWorkItemCounts[requirementID]
    }

    func prerequisites(for requirement: ProjectRequirement) -> [ProjectRequirement] {
        relatedRequirements(from: requirementPrerequisiteIDs[requirement.id] ?? [])
    }

    func dependents(for requirement: ProjectRequirement) -> [ProjectRequirement] {
        relatedRequirements(from: requirementDependentIDs[requirement.id] ?? [])
    }

    func executionScopeIDs(includePrerequisiteDependents: Bool) -> [String] {
        guard let rootID = selectedRequirementID else { return [] }
        var scope = Set(downstreamScopeIDs(rootID: rootID))
        var changed = true
        while changed {
            let before = scope.count
            for id in Array(scope) {
                for prerequisiteID in requirementPrerequisiteIDs[id] ?? [] {
                    guard let requirement = requirementByID[prerequisiteID],
                          !isDoneStatus(requirement.status) else { continue }
                    scope.insert(prerequisiteID)
                }
            }
            expandDescendants(in: &scope)
            if includePrerequisiteDependents {
                expandDependents(in: &scope)
                expandDescendants(in: &scope)
            }
            changed = scope.count != before
        }
        return orderedScope(rootID: rootID, scope: scope)
    }

    func downstreamScopeIDs(rootID: String) -> [String] {
        var scope: Set<String> = [rootID]
        expandDescendants(in: &scope)
        expandDependents(in: &scope)
        expandDescendants(in: &scope)
        return orderedScope(rootID: rootID, scope: scope)
    }

    func requirementScopeEdges(scopeIDs: [String]) -> [ProjectPlanEdge] {
        let scope = Set(scopeIDs)
        var result = requirementDependencyEdges.filter {
            scope.contains($0.sourceID) && scope.contains($0.targetID)
        }
        let existing = Set(result.map { "\($0.sourceID)->\($0.targetID)" })
        for requirement in requirements {
            guard scope.contains(requirement.id),
                  let parentID = normalized(requirement.parentRequirementID),
                  scope.contains(parentID),
                  !existing.contains("\(parentID)->\(requirement.id)") else { continue }
            result.append(ProjectPlanEdge(sourceID: parentID, targetID: requirement.id, kind: "child"))
        }
        return result
    }

    func dismissError() { errorMessage = nil }

    private var requirementPrerequisiteIDs: [String: [String]] {
        Dictionary(grouping: requirementDependencyEdges, by: \.targetID)
            .mapValues { $0.map(\.sourceID) }
    }

    private var requirementDependentIDs: [String: [String]] {
        Dictionary(grouping: requirementDependencyEdges, by: \.sourceID)
            .mapValues { $0.map(\.targetID) }
    }

    private func expandDescendants(in scope: inout Set<String>) {
        var changed = true
        while changed {
            let before = scope.count
            for requirement in requirements {
                guard let parentID = normalized(requirement.parentRequirementID),
                      scope.contains(parentID) else { continue }
                scope.insert(requirement.id)
            }
            changed = scope.count != before
        }
    }

    private func expandDependents(in scope: inout Set<String>) {
        var changed = true
        while changed {
            let before = scope.count
            for (sourceID, dependents) in requirementDependentIDs where scope.contains(sourceID) {
                scope.formUnion(dependents)
            }
            changed = scope.count != before
        }
    }

    private func orderedScope(rootID: String, scope: Set<String>) -> [String] {
        [rootID] + requirements.map(\.id).filter { $0 != rootID && scope.contains($0) }
    }

    private func relatedRequirements(from ids: [String]) -> [ProjectRequirement] {
        let lookup = requirementByID
        return ids.compactMap { lookup[$0] }
    }

    private func topologicallySorted(_ items: [ProjectWorkItem]) -> [ProjectWorkItem] {
        let byID = Dictionary(uniqueKeysWithValues: items.map { ($0.id, $0) })
        let ids = Set(byID.keys)
        let relevantEdges = edges.filter {
            ids.contains($0.sourceID) && ids.contains($0.targetID)
                && $0.kind.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() != "contains"
        }
        var indegree = Dictionary(uniqueKeysWithValues: items.map { ($0.id, 0) })
        var outgoing: [String: [String]] = [:]
        for edge in relevantEdges {
            indegree[edge.targetID, default: 0] += 1
            outgoing[edge.sourceID, default: []].append(edge.targetID)
        }
        var queue = items.filter { indegree[$0.id] == 0 }.map(\.id)
        var result: [ProjectWorkItem] = []
        var emitted = Set<String>()
        while !queue.isEmpty {
            let id = queue.removeFirst()
            guard emitted.insert(id).inserted, let item = byID[id] else { continue }
            result.append(item)
            for targetID in outgoing[id] ?? [] {
                indegree[targetID, default: 0] -= 1
                if indegree[targetID] == 0 { queue.append(targetID) }
            }
        }
        result.append(contentsOf: items.filter { !emitted.contains($0.id) })
        return result
    }

    private func normalized(_ value: String?) -> String? {
        let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? nil : trimmed
    }

    private func isDoneStatus(_ status: String) -> Bool {
        ["done", "completed", "succeeded", "success", "archived"]
            .contains(status.trimmingCharacters(in: .whitespacesAndNewlines).lowercased())
    }
}

private enum PlanLoadResult<Value: Sendable>: Sendable {
    case success(Value)
    case failure(String)
}

private func capturePlanLoad<Value: Sendable>(
    _ operation: @escaping @Sendable () async throws -> Value
) async -> PlanLoadResult<Value> {
    do {
        return .success(try await operation())
    } catch {
        return .failure(error.localizedDescription)
    }
}
