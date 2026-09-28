import ChatOSAPI
import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import AppKit
import Combine
import Foundation
import SwiftUI

@MainActor
extension AppModel {
    var currentConversationID: String? {
        switch selection {
        case .contact:
            return contactConversation?.sessionID
        case .project:
            return projectConversation?.sessionID
        default:
            return nil
        }
    }

    var interfaceLocale: Locale {
        interfaceLanguage.locale
    }

    func localized(_ chinese: String, english: String) -> String {
        interfaceLanguage == .english ? english : chinese
    }

    func toggleNavigationSidebar() {
        navigationSplitVisibility = navigationSplitVisibility == .detailOnly
            ? .all
            : .detailOnly
    }

    func startPetOverlayIfNeeded() {
        guard petOverlayCoordinator == nil else { return }
        petOverlayCoordinator = PetOverlayCoordinator(
            model: self,
            store: petOverlayStore,
            preferences: petPreferences
        )
    }

    func openPetFile(
        path: String,
        targetLine: Int? = nil,
        mode: PetFileOpenMode = .preview,
        access: PetFileAccess = .workspace
    ) {
        startPetOverlayIfNeeded()
        if !petPreferences.isEnabled {
            petPreferences.isEnabled = true
        }
        petOverlayCoordinator?.openFile(PetFileOpenRequest(
            path: path,
            targetLine: targetLine,
            mode: mode,
            access: access
        ))
    }

    func openUserSelectedPetFiles(_ urls: [URL]) {
        for url in urls where url.isFileURL {
            openPetFile(
                path: url.standardizedFileURL.path,
                access: .userSelectedLocal
            )
        }
    }

    func openPetTranslationImage(data: Data, suggestedName: String) {
        startPetOverlayIfNeeded()
        if !petPreferences.isEnabled {
            petPreferences.isEnabled = true
        }
        petOverlayCoordinator?.openTranslationImage(
            data: data,
            suggestedName: suggestedName
        )
    }

    @discardableResult
    func openPetFileLink(_ url: URL, projectRootPath: String?) -> Bool {
        guard let resolved = PetFileLinkResolver.resolve(
            url,
            projectRootPath: projectRootPath
        ) else { return false }
        openPetFile(path: resolved.path, targetLine: resolved.targetLine)
        return true
    }

    func startGlobalUtilitiesIfNeeded() {
        globalUtilityCoordinator.start()
    }

    func openPetActivity(_ activity: PetActivity?) {
        if activity?.source == .localApproval {
            openGlobalSearchSettings(tab: .approvals)
            return
        }

        var targetConversation: ConversationSessionViewModel?
        if let projectID = activity?.route.projectID,
           let project = projects.first(where: { $0.id == projectID }) {
            selection = .project(projectID)
            projectTab = .messages
            if let conversationID = activity?.route.conversationID ?? project.conversationID {
                targetConversation = conversation(for: conversationID)
                projectConversation = targetConversation
            }
        } else if let conversationID = activity?.route.conversationID {
            if let project = projects.first(where: { $0.conversationID == conversationID }) {
                selection = .project(project.id)
                projectTab = .messages
                targetConversation = conversation(for: conversationID)
                projectConversation = targetConversation
            } else if let contact = contacts.first(where: { $0.conversationID == conversationID }) {
                selection = .contact(contact.id)
                targetConversation = conversation(for: conversationID)
                contactConversation = targetConversation
            }
        }

        if let route = activity?.route {
            targetConversation?.focus(
                turnID: route.turnID,
                promptID: route.promptID,
                taskID: route.taskID,
                runID: route.runID
            )
        }

        showMainWindow()
    }

    func openGlobalSearchProject(_ projectID: String) {
        guard projects.contains(where: { $0.id == projectID }) else { return }
        selection = .project(projectID)
        projectTab = .messages
        showMainWindow()
    }

    func openGlobalSearchContact(_ contactID: String) {
        guard contacts.contains(where: { $0.id == contactID }) else { return }
        selection = .contact(contactID)
        showMainWindow()
    }

    func openGlobalSearchSettings(tab: LocalConnectorControlTab? = nil) {
        if let tab {
            requestConnectorSettings(tab)
        }
        if let settingsWindowPresentationHandler {
            settingsWindowPresentationHandler()
            return
        }
        NSApp.activate(ignoringOtherApps: true)
        if !NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil) {
            _ = NSApp.sendAction(Selector(("showPreferencesWindow:")), to: nil, from: nil)
        }
    }

    func showMainWindow() {
        if let mainWindowPresentationHandler {
            mainWindowPresentationHandler()
            return
        }
        NSApp.activate(ignoringOtherApps: true)
        let mainWindow = NSApp.windows.first {
            !($0 is NSPanel) && $0.title == "ChatOS"
        }
        mainWindow?.makeKeyAndOrderFront(nil)
    }

    func retryPetActivity(_ activity: PetActivity, instruction: String) async throws {
        guard let messageID = activity.route.messageID?.trimmingCharacters(in: .whitespacesAndNewlines),
              !messageID.isEmpty,
              let runID = activity.route.runID?.trimmingCharacters(in: .whitespacesAndNewlines),
              !runID.isEmpty else {
            throw PetActivityActionError.retryUnavailable
        }
        _ = try await messageTaskGraphService.retryRun(
            messageID: messageID,
            runID: runID,
            lookup: MessageTaskLookup(
                sessionID: activity.route.conversationID,
                turnID: activity.route.turnID
            ),
            instruction: instruction
        )
    }

    func cancelPetActivity(_ activity: PetActivity) async throws {
        if let messageID = activity.route.messageID?.trimmingCharacters(in: .whitespacesAndNewlines),
           !messageID.isEmpty,
           let taskID = activity.route.taskID?.trimmingCharacters(in: .whitespacesAndNewlines),
           !taskID.isEmpty {
            try await messageTaskGraphService.cancelTask(
                messageID: messageID,
                taskID: taskID,
                lookup: MessageTaskLookup(
                    sessionID: activity.route.conversationID,
                    turnID: activity.route.turnID
                ),
                reason: "用户从全局宠物面板取消任务"
            )
            return
        }

        if activity.source == .chat,
           let conversationID = activity.route.conversationID?
            .trimmingCharacters(in: .whitespacesAndNewlines),
           !conversationID.isEmpty,
           let turnID = activity.route.turnID?.trimmingCharacters(in: .whitespacesAndNewlines),
           !turnID.isEmpty {
            try await commandService.stopTurn(conversationID: conversationID, turnID: turnID)
            return
        }

        throw PetActivityActionError.cancelUnavailable
    }

    func loadPetTask(_ activity: PetActivity) async throws -> MessageTask {
        guard let messageID = activity.route.messageID?
            .trimmingCharacters(in: .whitespacesAndNewlines),
              !messageID.isEmpty,
              let taskID = activity.route.taskID?
                .trimmingCharacters(in: .whitespacesAndNewlines),
              !taskID.isEmpty else {
            throw PetActivityActionError.taskDetailUnavailable
        }
        let lookup = MessageTaskLookup(
            sessionID: activity.route.conversationID,
            turnID: activity.route.turnID
        )
        let task = try await messageTaskGraphService.fetchTask(
            messageID: messageID,
            taskID: taskID,
            lookup: lookup
        )
        let existingProcess = task.processLog?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if !existingProcess.isEmpty {
            return task
        }
        guard let runID = (activity.route.runID ?? task.lastRunID)?
            .trimmingCharacters(in: .whitespacesAndNewlines),
              !runID.isEmpty else {
            return task
        }
        let runDetail = try? await messageTaskGraphService.fetchRun(
            messageID: messageID,
            runID: runID,
            lookup: lookup,
            includeEvents: true,
            eventLimit: 40,
            eventOffset: 0
        )
        guard let runDetail else { return task }
        var mergedTask = runDetail.task.merging(run: runDetail.run)
        let mergedProcess = mergedTask.processLog?
            .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if mergedProcess.isEmpty, !runDetail.events.isEmpty {
            let formatter = ISO8601DateFormatter()
            mergedTask.processLog = runDetail.events.map { event in
                let timestamp = event.createdAt.map(formatter.string(from:)) ?? "事件"
                let title = event.eventType.trimmingCharacters(in: .whitespacesAndNewlines)
                let detail = event.message?
                    .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                return "[\(timestamp)] \(title.isEmpty ? "过程更新" : title)\n"
                    + (detail.isEmpty ? "已记录该执行事件" : detail)
            }
            .joined(separator: "\n")
        }
        return mergedTask
    }

    func loadPetAskUserPrompt(_ activity: PetActivity) async throws -> AskUserPrompt {
        guard let sessionID = activity.route.conversationID?
            .trimmingCharacters(in: .whitespacesAndNewlines),
              !sessionID.isEmpty,
              let promptID = activity.route.promptID?
                .trimmingCharacters(in: .whitespacesAndNewlines),
              !promptID.isEmpty else {
            throw PetActivityActionError.promptUnavailable
        }
        let prompts = try await askUserPromptService.fetchPrompts(sessionID: sessionID, limit: 100)
        guard let prompt = prompts.first(where: { $0.id == promptID && $0.status.isPending }) else {
            throw PetActivityActionError.promptResolved
        }
        return prompt
    }

    func submitPetAskUserPrompt(
        _ prompt: AskUserPrompt,
        submission: AskUserSubmission
    ) async throws {
        _ = try await askUserPromptService.submit(
            promptID: prompt.id,
            sessionID: prompt.sessionID,
            submission: submission
        )
    }

    func cancelPetAskUserPrompt(_ prompt: AskUserPrompt) async throws {
        _ = try await askUserPromptService.cancel(
            promptID: prompt.id,
            sessionID: prompt.sessionID
        )
    }

    func applyPetActivityDisposition(
        _ disposition: PetActivityDisposition,
        to activity: PetActivity
    ) async throws {
        try await petActivityInboxService.apply(disposition, to: activity)
    }

    func recoverPetActivities() async throws -> [PetActivity] {
        try await petActivityInboxService.fetchOpenActivities(limit: 500)
    }

}
