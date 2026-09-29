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
        hasSelectedConversation: Bool,
        isSelectedSessionExpanded: Bool
    ) -> Duration {
        if !hasSessions { return .seconds(5) }
        if !hasSelectedConversation || !isSelectedSessionExpanded {
            return .seconds(15)
        }
        return .milliseconds(450)
    }

    static func shouldLoadFrameData(
        hasSelectedConversation: Bool,
        isSelectedSessionExpanded: Bool
    ) -> Bool {
        hasSelectedConversation && isSelectedSessionExpanded
    }
}

@MainActor
extension AppModel {
    func startVisualSessionMonitoring() {
        guard visualSessionMonitorTask == nil,
              NSApplication.shared.isActive else { return }
        let service = localConnectorService
        visualSessionMonitorTask = Task { [weak self] in
            while !Task.isCancelled {
                let hasSelectedConversation = self?.currentConversationID != nil
                let selectedPresentation = hasSelectedConversation
                    ? self?.visualSessionStore.selectedPresentation
                    : nil
                let isSelectedSessionExpanded = selectedPresentation?.isExpanded == true
                let selectedAdapterSessionID = VisualSessionPollingPolicy.shouldLoadFrameData(
                    hasSelectedConversation: hasSelectedConversation,
                    isSelectedSessionExpanded: isSelectedSessionExpanded
                ) ? selectedPresentation?.session.adapterSessionID : nil
                let preferredAdapterSessionIDs = selectedAdapterSessionID.map { Set([$0]) } ?? []
                let existingFrames = Dictionary(uniqueKeysWithValues:
                    (self?.visualSessionStore.presentations ?? []).compactMap { presentation in
                        presentation.frameImage.map {
                            (VisualSessionFrameIdentity(
                                adapterSessionID: presentation.session.adapterSessionID,
                                frameSequence: presentation.session.frameSequence
                            ), $0)
                        }
                    }
                )
                let knownFrameSequences = Dictionary(uniqueKeysWithValues:
                    existingFrames.keys.map { ($0.adapterSessionID, $0.frameSequence) }
                )
                let sessions = await service.fetchPluginVisualSessions(
                    loadFrameDataForAdapterSessionIDs: preferredAdapterSessionIDs,
                    knownFrameSequencesByAdapterSessionID: knownFrameSequences
                )
                guard !Task.isCancelled else { return }
                let preparedSessions = await VisualSessionFrameDecoder.prepare(
                    sessions,
                    reusing: existingFrames
                )
                guard !Task.isCancelled else { return }
                self?.applyPluginVisualSessions(preparedSessions)
                let nextHasSelectedConversation = self?.currentConversationID != nil
                let nextIsSelectedSessionExpanded = nextHasSelectedConversation
                    && self?.visualSessionStore.selectedPresentation?.isExpanded == true
                let interval = VisualSessionPollingPolicy.interval(
                    hasSessions: !sessions.isEmpty,
                    hasSelectedConversation: nextHasSelectedConversation,
                    isSelectedSessionExpanded: nextIsSelectedSessionExpanded
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

    func applyPluginVisualSessions(_ preparedSessions: [PreparedVisualSession]) {
        guard let conversationID = currentConversationID else {
            visualSessionStore.update([], selectedAdapterSessionID: nil)
            return
        }

        let previousPresentations = Dictionary(uniqueKeysWithValues:
            visualSessionStore.presentations.map { ($0.session.adapterSessionID, $0) }
        )
        let matchingSessions = preparedSessions
            .filter { $0.session.owner.conversationID == conversationID }
            .sorted { lhs, rhs in
                let lhsDate = lhs.session.capturedAt ?? .distantPast
                let rhsDate = rhs.session.capturedAt ?? .distantPast
                if lhsDate != rhsDate { return lhsDate > rhsDate }
                return lhs.session.adapterSessionID < rhs.session.adapterSessionID
            }

        let presentations = matchingSessions.map { incoming -> VisualSessionPresentation in
            let session = incoming.session
            let previous = previousPresentations[session.adapterSessionID]
            let frameImage = incoming.frameImage
                ?? previous.flatMap {
                    $0.session.frameSequence == session.frameSequence ? $0.frameImage : nil
                }
            let key = session.adapterSessionID
            let isExpanded = visualSessionExpansion[key]
                ?? previous?.isExpanded
                ?? true
            visualSessionExpansion[key] = isExpanded
            return .init(
                session: session,
                isExpanded: isExpanded,
                frameImage: frameImage
            )
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

        let activeKeys = Set(preparedSessions.map(\.session.adapterSessionID))
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
