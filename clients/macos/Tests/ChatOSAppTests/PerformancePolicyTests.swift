import ChatOSCore
import Foundation
import Testing
@testable import ChatOSApp

@Suite("Performance policies")
struct PerformancePolicyTests {
    @Test("conversation recency is bounded and refreshes recently used entries")
    func conversationRecencyIsBounded() {
        var recency = ConversationCacheRecency(capacity: 3)
        #expect(recency.touch("a").isEmpty)
        #expect(recency.touch("b").isEmpty)
        #expect(recency.touch("c").isEmpty)
        #expect(recency.touch("a").isEmpty)
        #expect(recency.touch("d") == ["b"])
        #expect(recency.sessionIDs == ["c", "a", "d"])
    }

    @Test("conversation recency does not evict protected sessions")
    func conversationRecencyProtectsActiveSession() {
        var recency = ConversationCacheRecency(capacity: 2)
        _ = recency.touch("active")
        _ = recency.touch("old")

        #expect(recency.touch("new", protected: ["active", "new"]) == ["old"])
        #expect(recency.sessionIDs == ["active", "new"])
    }

    @Test("thirty conversation visits retain only the eight most recent")
    func thirtyConversationVisitsStayBounded() {
        var recency = ConversationCacheRecency(capacity: 8)
        var evicted: [String] = []
        for index in 0..<30 {
            evicted.append(contentsOf: recency.touch("session-\(index)"))
        }

        #expect(recency.sessionIDs.count == 8)
        #expect(recency.sessionIDs == (22..<30).map { "session-\($0)" })
        #expect(evicted == (0..<22).map { "session-\($0)" })
    }

    @Test("visual session polling backs off when idle")
    func visualSessionPollingBacksOff() {
        #expect(VisualSessionPollingPolicy.interval(
            hasSessions: false,
            hasSelectedConversation: true,
            isSelectedSessionExpanded: true
        ) == .seconds(5))
        #expect(VisualSessionPollingPolicy.interval(
            hasSessions: true,
            hasSelectedConversation: false,
            isSelectedSessionExpanded: true
        ) == .seconds(2))
        #expect(VisualSessionPollingPolicy.interval(
            hasSessions: true,
            hasSelectedConversation: true,
            isSelectedSessionExpanded: false
        ) == .seconds(2))
        #expect(VisualSessionPollingPolicy.interval(
            hasSessions: true,
            hasSelectedConversation: true,
            isSelectedSessionExpanded: true
        ) == .milliseconds(450))
        #expect(!VisualSessionPollingPolicy.shouldLoadFrameData(
            hasSelectedConversation: true,
            isSelectedSessionExpanded: false
        ))
        #expect(VisualSessionPollingPolicy.shouldLoadFrameData(
            hasSelectedConversation: true,
            isSelectedSessionExpanded: true
        ))
    }

    @Test("approval events replace two-second consistency polling")
    func approvalMonitoringUsesEventStreams() {
        #expect(LocalConnectorApprovalMonitoringPolicy.consistencyCheckInterval(
            hasStreamingService: true
        ) == .seconds(60))
        #expect(LocalConnectorApprovalMonitoringPolicy.consistencyCheckInterval(
            hasStreamingService: false
        ) == .seconds(2))
    }

    @Test("clipboard polling backs off and stays slower in background")
    func clipboardPollingBacksOff() {
        #expect(ClipboardPollingPolicy.interval(
            isApplicationActive: true,
            idlePollCount: 0
        ) == .milliseconds(300))
        #expect(ClipboardPollingPolicy.interval(
            isApplicationActive: true,
            idlePollCount: 20
        ) == .milliseconds(1_500))
        #expect(ClipboardPollingPolicy.interval(
            isApplicationActive: false,
            idlePollCount: 20
        ) == .seconds(3))
    }

    @Test("clipboard payload preparation is bounded and deterministic")
    func clipboardPayloadPreparationIsBounded() {
        let value = "  first\nsecond  "
        let prepared = ClipboardPayloadPreparation.prepare(
            .text(value),
            maximumPayloadBytes: 1_024
        )
        let repeated = ClipboardPayloadPreparation.prepare(
            .text(value),
            maximumPayloadBytes: 1_024
        )

        #expect(prepared?.payload == .text(value))
        #expect(prepared?.preview == "first second")
        #expect(prepared?.hash == repeated?.hash)
        #expect(ClipboardPayloadPreparation.prepare(
            .image(data: Data(repeating: 1, count: 8), pasteboardType: "public.png"),
            maximumPayloadBytes: 4
        ) == nil)
    }

    @Test("project run monitoring sleeps once all instances stop")
    func projectRunMonitoringStopsRefreshingIdleInstances() {
        let stopped = ProjectRunInstance(
            id: "stopped",
            name: "Stopped",
            cwd: nil,
            status: "stopped",
            isBusy: false,
            isRunning: false
        )
        let running = ProjectRunInstance(
            id: "running",
            name: "Running",
            cwd: nil,
            status: "running",
            isBusy: false,
            isRunning: true
        )

        let stoppedState = ProjectRunState(
            projectID: "project",
            status: "idle",
            isBusy: false,
            isRunning: false,
            instances: [stopped]
        )
        let runningState = ProjectRunState(
            projectID: "project",
            status: "running",
            isBusy: false,
            isRunning: true,
            instances: [running]
        )

        #expect(ProjectRunMonitoringPolicy.shouldRefresh(nil) == false)
        #expect(ProjectRunMonitoringPolicy.shouldRefresh(stoppedState) == false)
        #expect(ProjectRunMonitoringPolicy.shouldRefresh(runningState))
    }

    @Test("application activation keeps an existing artifact sync coordinator")
    func applicationActivationKeepsArtifactSyncCoordinator() {
        #expect(!AgentArtifactSyncCoordinatorPolicy.shouldStart(
            existingOwnerUserID: "owner",
            requestedOwnerUserID: "owner",
            hasLiveTask: true,
            forceRestart: false
        ))
        #expect(AgentArtifactSyncCoordinatorPolicy.shouldStart(
            existingOwnerUserID: "old-owner",
            requestedOwnerUserID: "new-owner",
            hasLiveTask: true,
            forceRestart: false
        ))
        #expect(AgentArtifactSyncCoordinatorPolicy.shouldStart(
            existingOwnerUserID: "owner",
            requestedOwnerUserID: "owner",
            hasLiveTask: true,
            forceRestart: true
        ))
    }

    @Test("application activation coalesces connector recovery")
    func applicationActivationCoalescesConnectorRecovery() {
        #expect(!LocalConnectorRecoveryPolicy.shouldStart(
            forceReconnect: false,
            hasLiveTask: true,
            secondsSinceLastRecovery: nil
        ))
        #expect(!LocalConnectorRecoveryPolicy.shouldStart(
            forceReconnect: false,
            hasLiveTask: false,
            secondsSinceLastRecovery: 10
        ))
        #expect(LocalConnectorRecoveryPolicy.shouldStart(
            forceReconnect: false,
            hasLiveTask: false,
            secondsSinceLastRecovery: 60
        ))
        #expect(LocalConnectorRecoveryPolicy.shouldStart(
            forceReconnect: true,
            hasLiveTask: true,
            secondsSinceLastRecovery: 1
        ))
    }

    @Test("collapsed task cards normalize and bound long text")
    func collapsedTaskCardTextIsBounded() {
        let input = "  first\n\nsecond   " + String(repeating: "界", count: 240)
        let summary = TeamTodoCardText.collapsedSummary(input, maximumCharacters: 40)

        #expect(!summary.contains("\n"))
        #expect(!summary.contains("  "))
        #expect(summary.count == 41)
        #expect(summary.hasSuffix("…"))
        #expect(TeamTodoCardText.collapsedSummary(" short text ") == "short text")
    }
}
