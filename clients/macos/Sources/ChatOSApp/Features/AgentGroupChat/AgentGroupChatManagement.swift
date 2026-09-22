import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation

@MainActor
extension AgentGroupChatViewModel {
    func prepareAgentEditor() async -> Bool {
        if hasLoadedModels {
            if availableModels.isEmpty {
                errorMessage = LocalAgentBuilderError.noAvailableModel.localizedDescription
                return false
            }
            return true
        }
        let task: Task<LocalAgentBuilderResources, Error>
        if let modelLoadTask {
            task = modelLoadTask
        } else {
            let builderService = builderService
            let ownerUserID = ownerUserID
            let created = Task {
                try await builderService.loadResources(ownerUserID: ownerUserID)
            }
            modelLoadTask = created
            task = created
        }
        isLoadingModels = true
        defer {
            isLoadingModels = false
            modelLoadTask = nil
        }
        do {
            availableModels = try await task.value.models
            hasLoadedModels = true
            guard !availableModels.isEmpty else {
                errorMessage = LocalAgentBuilderError.noAvailableModel.localizedDescription
                return false
            }
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    @discardableResult
    func loadOlderMessages() async -> String? {
        guard !isLoadingOlderMessages,
              hasOlderMessages,
              let room,
              let firstMessageID = messages.first?.id else { return nil }
        isLoadingOlderMessages = true
        defer { isLoadingOlderMessages = false }
        do {
            let store = try await resolveStore()
            let page = try await store.pageRecentMessages(
                ownerUserID: ownerUserID,
                roomID: room.id,
                beforeMessageID: firstMessageID,
                limit: messagePageSize
            )
            messages = mergeMessages(messages, with: page.messages)
            hasOlderMessages = page.hasMore
            let loadedAttachmentData = try await loadAttachmentData(
                messages: page.messages,
                roomID: room.id,
                store: store
            )
            attachmentDataByID.merge(loadedAttachmentData) { _, new in new }
            errorMessage = nil
            return firstMessageID
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func activate() async {
        await load()
        startChangeObservation()
        if room != nil {
            startScheduler()
        }
    }

    func startChangeObservation() {
        guard changeObservationTask == nil else { return }
        let service = service
        let ownerUserID = ownerUserID
        changeObservationTask = Task { [weak self] in
            let changes = await service.changes(ownerUserID: ownerUserID)
            let refreshCoalescer = AgentChangeRefreshCoalescer { [weak self] in
                await self?.load()
            }
            defer { refreshCoalescer.cancel() }
            for await _ in changes {
                guard !Task.isCancelled else { break }
                refreshCoalescer.signal()
            }
        }
    }

    func saveTeamAsset(
        existing: LocalAgentTeamAsset?,
        category: LocalAgentTeamAssetCategory,
        title: String,
        markdown: String
    ) async -> Bool {
        guard let room else { return false }
        do {
            let store = try await resolveStore()
            let saved = try await store.upsertTeamAsset(
                ownerUserID: ownerUserID,
                teamRoomID: room.id,
                assetID: existing?.id,
                editorAgentID: nil,
                category: category,
                title: title.trimmingCharacters(in: .whitespacesAndNewlines),
                markdown: markdown,
                expectedRevision: existing?.revision,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            teamAssetRevisions.removeValue(forKey: saved.id)
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func archiveTeamAsset(_ asset: LocalAgentTeamAsset) async {
        guard let room else { return }
        do {
            let store = try await resolveStore()
            _ = try await store.archiveTeamAsset(
                ownerUserID: ownerUserID,
                teamRoomID: room.id,
                assetID: asset.id,
                editorAgentID: nil,
                expectedRevision: asset.revision,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            teamAssetRevisions.removeValue(forKey: asset.id)
            await load()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func submitRequirementSurvey(
        _ survey: LocalAgentRequirementSurvey,
        selections: [String: Set<String>],
        notes: String
    ) async -> Bool {
        guard let room, survey.projectID == projectID,
              submittingRequirementSurveyIDs.insert(survey.id).inserted else { return false }
        defer { submittingRequirementSurveyIDs.remove(survey.id) }
        do {
            let answers = survey.draft.questions.compactMap { question -> LocalAgentRequirementSurveyAnswer? in
                let selected = selections[question.id] ?? []
                guard !selected.isEmpty else { return nil }
                let ordered = question.options.map(\.id).filter(selected.contains)
                return .init(questionID: question.id, selectedOptionIDs: ordered)
            }
            let normalizedNotes = notes.trimmingCharacters(in: .whitespacesAndNewlines)
            let store = try await resolveStore()
            _ = try await store.submitRequirementSurvey(
                ownerUserID: ownerUserID,
                projectID: projectID,
                surveyID: survey.id,
                submission: .init(answers: answers, notes: normalizedNotes),
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await service.publishChange(.init(
                ownerUserID: ownerUserID,
                roomID: room.id,
                kind: .roomUpdated
            ))
            await load()
            startScheduler()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func loadTeamAssetRevisions(_ asset: LocalAgentTeamAsset, force: Bool = false) async {
        guard let room,
              asset.teamRoomID == room.id,
              force || teamAssetRevisions[asset.id] == nil,
              loadingTeamAssetRevisionIDs.insert(asset.id).inserted else { return }
        defer { loadingTeamAssetRevisionIDs.remove(asset.id) }
        do {
            let store = try await resolveStore()
            let revisions = try await store.listTeamAssetRevisions(
                ownerUserID: ownerUserID,
                teamRoomID: room.id,
                assetID: asset.id,
                limit: 500
            )
            guard self.room?.id == room.id,
                  teamAssets.contains(where: { $0.id == asset.id }) else { return }
            teamAssetRevisions[asset.id] = revisions
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func createRoom(
        name: String,
        goal: String,
        projectManagerAgentID: String
    ) async -> Bool {
        do {
            let store = try await resolveStore()
            _ = try await store.createManagedRoom(
                ownerUserID: ownerUserID,
                projectID: projectID,
                draft: .init(
                    name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                    goal: goal.trimmingCharacters(in: .whitespacesAndNewlines)
                ),
                projectManagerAgentID: projectManagerAgentID
            )
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func createAgentAndJoin(
        name: String,
        avatarData: Data?,
        role: String,
        responsibility: String,
        rolePrompt: String,
        modelConfigID: String,
        thinkingLevel: String?,
        professionKey: String
    ) async -> Bool {
        guard let room else {
            errorMessage = AgentGroupChatError.notFound.localizedDescription
            return false
        }
        let normalizedModelConfigID = modelConfigID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let selectedModel = availableModels.first(where: { $0.id == normalizedModelConfigID }) else {
            errorMessage = LocalAgentBuilderError.modelUnavailable.localizedDescription
            return false
        }
        let normalizedThinkingLevel = LocalAgentThinkingLevelCatalog.normalized(
            thinkingLevel,
            allowedValues: selectedModel.thinkingLevels
        )
        if let thinkingLevel = thinkingLevel?.trimmingCharacters(in: .whitespacesAndNewlines),
           !thinkingLevel.isEmpty, normalizedThinkingLevel == nil {
            errorMessage = AgentGroupChatError.invalidField("thinkingLevel").localizedDescription
            return false
        }
        do {
            let store = try await resolveStore()
            let agent = try await store.createAgent(
                ownerUserID: ownerUserID,
                draft: .init(
                    name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                    avatarData: avatarData,
                    description: responsibility.trimmingCharacters(in: .whitespacesAndNewlines),
                    rolePrompt: rolePrompt.trimmingCharacters(in: .whitespacesAndNewlines),
                    modelConfigID: normalizedModelConfigID,
                    thinkingLevel: normalizedThinkingLevel,
                    professionKey: professionKey,
                    defaultPluginIDs: []
                )
            )
            _ = try await store.addMember(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: agent.id,
                draft: .init(
                    role: role.trimmingCharacters(in: .whitespacesAndNewlines),
                    responsibility: responsibility.trimmingCharacters(in: .whitespacesAndNewlines),
                    pluginAllowlist: []
                )
            )
            if members.isEmpty {
                _ = try await store.setDefaultAgent(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    agentID: agent.id
                )
            }
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func inviteAgent(agentID: String) async -> Bool {
        guard let room,
              let profile = profilesByID[agentID],
              !members.contains(where: { $0.agentID == agentID }) else {
            errorMessage = AgentGroupChatError.conflict.localizedDescription
            return false
        }
        do {
            let store = try await resolveStore()
            _ = try await store.addMember(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: agentID,
                draft: .init(
                    role: profile.draft.name,
                    responsibility: profile.draft.description,
                    pluginAllowlist: []
                )
            )
            if members.isEmpty {
                _ = try await store.setDefaultAgent(
                    ownerUserID: ownerUserID,
                    roomID: room.id,
                    agentID: agentID
                )
            }
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func updateAgentMembership(
        agentID: String,
        name: String,
        avatarData: Data?,
        role: String,
        responsibility: String,
        rolePrompt: String,
        modelConfigID: String,
        thinkingLevel: String?
    ) async -> Bool {
        guard let room, let existingProfile = profilesByID[agentID] else {
            errorMessage = AgentGroupChatError.notFound.localizedDescription
            return false
        }
        let normalizedModelConfigID = modelConfigID.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let selectedModel = availableModels.first(where: { $0.id == normalizedModelConfigID }) else {
            errorMessage = LocalAgentBuilderError.modelUnavailable.localizedDescription
            return false
        }
        let normalizedThinkingLevel = LocalAgentThinkingLevelCatalog.normalized(
            thinkingLevel,
            allowedValues: selectedModel.thinkingLevels
        )
        if let thinkingLevel = thinkingLevel?.trimmingCharacters(in: .whitespacesAndNewlines),
           !thinkingLevel.isEmpty, normalizedThinkingLevel == nil {
            errorMessage = AgentGroupChatError.invalidField("thinkingLevel").localizedDescription
            return false
        }
        do {
            let store = try await resolveStore()
            _ = try await store.updateAgentMembership(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: agentID,
                profileDraft: .init(
                    name: name.trimmingCharacters(in: .whitespacesAndNewlines),
                    avatarData: avatarData,
                    description: responsibility.trimmingCharacters(in: .whitespacesAndNewlines),
                    rolePrompt: rolePrompt.trimmingCharacters(in: .whitespacesAndNewlines),
                    modelConfigID: normalizedModelConfigID,
                    thinkingLevel: normalizedThinkingLevel,
                    professionKey: existingProfile.draft.professionKey,
                    defaultPluginIDs: [],
                    defaultSkillIDs: existingProfile.draft.defaultSkillIDs
                ),
                memberDraft: .init(
                    role: role.trimmingCharacters(in: .whitespacesAndNewlines),
                    responsibility: responsibility.trimmingCharacters(in: .whitespacesAndNewlines),
                    pluginAllowlist: []
                )
            )
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func setProjectManager(agentID: String) async -> Bool {
        guard let room,
              let profile = profilesByID[agentID],
              profile.draft.professionKey == "project_manager",
              members.contains(where: { $0.agentID == agentID && $0.status == .active }) else {
            errorMessage = AgentGroupChatError.invalidField(
                "projectManagerProfession"
            ).localizedDescription
            return false
        }
        do {
            let store = try await resolveStore()
            _ = try await store.setProjectManager(
                ownerUserID: ownerUserID,
                roomID: room.id,
                agentID: agentID
            )
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func generateAgentDraft(brief: String, builderModelConfigID: String) async -> LocalAgentDraft? {
        do {
            let draft = try await builderService.generateDraft(
                ownerUserID: ownerUserID,
                projectID: projectID,
                brief: brief,
                builderModelConfigID: builderModelConfigID
            )
            errorMessage = nil
            return draft
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func confirmAgentDraft(_ draft: LocalAgentDraft) async -> Bool {
        do {
            _ = try await builderService.createConfirmedDraft(
                ownerUserID: ownerUserID,
                projectID: projectID,
                draft: draft
            )
            await load()
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func approveProposal(_ proposal: LocalAgentCreationProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            _ = try await builderService.approveProposal(
                ownerUserID: ownerUserID,
                projectID: projectID,
                proposal: proposal
            )
            await load()
            startScheduler()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func rejectProposal(_ proposal: LocalAgentCreationProposal) async {
        guard proposalActionIDs.insert(proposal.id).inserted else { return }
        defer { proposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectAgentProposal(
                ownerUserID: ownerUserID,
                roomID: proposal.roomID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
            startScheduler()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func approveRemovalProposal(_ proposal: LocalAgentRemovalProposal) async {
        guard removalProposalActionIDs.insert(proposal.id).inserted else { return }
        defer { removalProposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.approveAgentRemovalProposal(
                ownerUserID: ownerUserID,
                roomID: proposal.roomID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
            startScheduler()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func rejectRemovalProposal(_ proposal: LocalAgentRemovalProposal) async {
        guard removalProposalActionIDs.insert(proposal.id).inserted else { return }
        defer { removalProposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectAgentRemovalProposal(
                ownerUserID: ownerUserID,
                roomID: proposal.roomID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
            startScheduler()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func approveTeamProposal(
        _ proposal: LocalAgentTeamCreationProposal
    ) async -> WorkspaceProject? {
        guard teamProposalActionIDs.insert(proposal.id).inserted else { return nil }
        defer { teamProposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            let createdProject: WorkspaceProject?
            let resolvedProjectID: String
            if let existingProjectID = proposal.draft.existingProjectID {
                let registry = try await projectsService.registry()
                guard let project = try await registry.get(
                    ownerUserID: ownerUserID,
                    id: existingProjectID
                ), project.status == .active else {
                    throw ProjectRegistryError.notFound
                }
                createdProject = nil
                resolvedProjectID = project.id
            } else if let importedDraft = proposal.draft.importedProjectDraft,
                      let absolutePath = proposal.draft.importedProjectAbsolutePath {
                let project = try await projectsService.createFromExistingDirectory(
                    ownerUserID: ownerUserID,
                    draft: importedDraft,
                    absolutePath: absolutePath
                )
                createdProject = project
                resolvedProjectID = project.id
            } else if let newProjectName = proposal.draft.newProjectName {
                let project = try await projectsService.createInDefaultWorkspace(
                    ownerUserID: ownerUserID,
                    name: newProjectName,
                    description: proposal.draft.newProjectDescription,
                    projectTypeKey: proposal.draft.newProjectTypeKey
                        ?? LocalAgentSkillCatalog.legacyProjectTypeKey
                )
                createdProject = project
                resolvedProjectID = project.id
            } else {
                throw AgentGroupChatError.conflict
            }
            _ = try await store.approveTeamProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: proposal.sourceRoomID,
                proposalID: proposal.id,
                resolvedProjectID: resolvedProjectID,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
            NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            startScheduler()
            return createdProject
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func rejectTeamProposal(_ proposal: LocalAgentTeamCreationProposal) async {
        guard teamProposalActionIDs.insert(proposal.id).inserted else { return }
        defer { teamProposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectTeamProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: proposal.sourceRoomID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
            startScheduler()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func approveMembershipProposal(_ proposal: LocalAgentMembershipProposal) async {
        guard membershipProposalActionIDs.insert(proposal.id).inserted else { return }
        defer { membershipProposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.approveMembershipProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: proposal.sourceRoomID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
            NotificationCenter.default.post(name: .agentGroupChatRoomsDidChange, object: nil)
            startScheduler()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func rejectMembershipProposal(_ proposal: LocalAgentMembershipProposal) async {
        guard membershipProposalActionIDs.insert(proposal.id).inserted else { return }
        defer { membershipProposalActionIDs.remove(proposal.id) }
        do {
            let store = try await resolveStore()
            _ = try await store.rejectMembershipProposal(
                ownerUserID: ownerUserID,
                sourceRoomID: proposal.sourceRoomID,
                proposalID: proposal.id,
                nowUnixMs: Int64(Date().timeIntervalSince1970 * 1_000)
            )
            await load()
            startScheduler()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

}
