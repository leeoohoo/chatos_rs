import ChatOSAPI
import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import AppKit
import Combine
import Foundation
import SwiftUI

enum VisualSessionPollingPolicy {
    static func interval(
        hasSessions: Bool,
        hasSelectedConversation: Bool
    ) -> Duration {
        if !hasSessions { return .seconds(5) }
        if !hasSelectedConversation { return .milliseconds(1_500) }
        return .milliseconds(450)
    }
}

@MainActor
extension AppModel {
    func startVisualSessionMonitoring() {
        guard visualSessionMonitorTask == nil else { return }
        let service = localConnectorService
        visualSessionMonitorTask = Task { [weak self] in
            while !Task.isCancelled {
                let hasSelectedConversation = self?.currentConversationID != nil
                let selectedAdapterSessionID = hasSelectedConversation
                    ? self?.visualSessionStore.selectedAdapterSessionID
                    : nil
                let preferredAdapterSessionIDs = selectedAdapterSessionID.map { Set([$0]) } ?? []
                let sessions = await service.fetchPluginVisualSessions(
                    loadFrameDataForAdapterSessionIDs: preferredAdapterSessionIDs
                )
                guard !Task.isCancelled else { return }
                self?.applyPluginVisualSessions(sessions)
                let interval = VisualSessionPollingPolicy.interval(
                    hasSessions: !sessions.isEmpty,
                    hasSelectedConversation: hasSelectedConversation
                )
                do {
                    try await Task.sleep(for: interval)
                } catch {
                    return
                }
            }
        }
    }

    func stopVisualSessionMonitoring() {
        visualSessionMonitorTask?.cancel()
        visualSessionMonitorTask = nil
    }

    var interfaceDynamicTypeSize: DynamicTypeSize {
        switch Int(interfaceFontSize.rounded()) {
        case ...12: .small
        case 13: .medium
        case 14: .large
        case 15: .xLarge
        case 16: .xxLarge
        case 17: .xxxLarge
        default: .accessibility1
        }
    }

    func toggleVisualSession() {
        guard var visualSession = visualSessionStore.selectedPresentation else { return }
        visualSession.isExpanded.toggle()
        visualSessionExpansion[visualSession.session.adapterSessionID] = visualSession.isExpanded
        visualSessionStore.updatePresentation(visualSession)
    }

    func selectPreviousVisualSession() {
        selectVisualSession(offset: -1)
    }

    func selectNextVisualSession() {
        selectVisualSession(offset: 1)
    }

    func applyPluginVisualSessions(_ sessions: [PluginVisualSession]) {
        guard let conversationID = currentConversationID else {
            visualSessionStore.update([], selectedAdapterSessionID: nil)
            return
        }

        let previousPresentations = Dictionary(uniqueKeysWithValues:
            visualSessionStore.presentations.map { ($0.session.adapterSessionID, $0) }
        )
        let matchingSessions = sessions
            .filter { $0.owner.conversationID == conversationID }
            .sorted { lhs, rhs in
                let lhsDate = lhs.capturedAt ?? .distantPast
                let rhsDate = rhs.capturedAt ?? .distantPast
                if lhsDate != rhsDate { return lhsDate > rhsDate }
                return lhs.adapterSessionID < rhs.adapterSessionID
            }

        let presentations = matchingSessions.map { incoming -> VisualSessionPresentation in
            var session = incoming
            if session.frameData == nil,
               let previous = previousPresentations[session.adapterSessionID],
               previous.session.frameSequence == session.frameSequence {
                session.frameData = previous.session.frameData
            }
            let key = session.adapterSessionID
            let isExpanded = visualSessionExpansion[key]
                ?? previousPresentations[key]?.isExpanded
                ?? true
            visualSessionExpansion[key] = isExpanded
            return .init(session: session, isExpanded: isExpanded)
        }

        let activeAdapterSessionIDs = Set(presentations.map(\.session.adapterSessionID))
        let rememberedSelection = visualSessionSelection[conversationID]
            ?? visualSessionStore.selectedAdapterSessionID
        let selectedAdapterSessionID = rememberedSelection.flatMap { candidate in
            activeAdapterSessionIDs.contains(candidate) ? candidate : nil
        } ?? presentations.first?.session.adapterSessionID
        visualSessionSelection[conversationID] = selectedAdapterSessionID
        visualSessionStore.update(
            presentations,
            selectedAdapterSessionID: selectedAdapterSessionID
        )

        let activeKeys = Set(sessions.map(\.adapterSessionID))
        visualSessionExpansion = visualSessionExpansion.filter { activeKeys.contains($0.key) }
    }

    func selectVisualSession(offset: Int) {
        let presentations = visualSessionStore.presentations
        guard presentations.count > 1 else { return }
        let currentIndex = visualSessionStore.selectedIndex ?? 0
        let nextIndex = (currentIndex + offset + presentations.count) % presentations.count
        let nextAdapterSessionID = presentations[nextIndex].session.adapterSessionID
        visualSessionStore.select(adapterSessionID: nextAdapterSessionID)
        if let conversationID = currentConversationID {
            visualSessionSelection[conversationID] = nextAdapterSessionID
        }
    }

}
