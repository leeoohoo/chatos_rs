import ChatOSCore
import ChatOSConnector
import Foundation
import Testing
@testable import ChatOSApp

@Suite("Performance policies")
struct PerformancePolicyTests {
    @Test("agent workspace coalesces changes into the minimum refresh scope")
    func agentWorkspaceCoalescesChangeRefreshes() {
        var plan = AgentWorkspaceRefreshPlan()
        let runID = UUID()
        plan.record(.init(
            ownerUserID: "alice",
            roomID: "room-1",
            runID: runID,
            kind: .runUpdated
        ))
        #expect(!plan.reloadWorkspace)
        #expect(!plan.reloadTriggerRuns)
        #expect(plan.updatedRunIDs == [runID])

        plan.record(.init(
            ownerUserID: "alice",
            roomID: "room-1",
            kind: .deliveryClaimed
        ))
        #expect(plan.reloadWorkspace)
        #expect(plan.reloadTriggerRuns)
        let consumed = plan.take()
        #expect(consumed.reloadWorkspace)
        #expect(consumed.reloadTriggerRuns)
        #expect(consumed.updatedRunIDs == [runID])
        #expect(plan == .init())
    }

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
        ) == .seconds(1_800))
        #expect(VisualSessionPollingPolicy.interval(
            hasSessions: true,
            hasSelectedConversation: false,
            isSelectedSessionExpanded: true
        ) == .seconds(15))
        #expect(VisualSessionPollingPolicy.interval(
            hasSessions: true,
            hasSelectedConversation: true,
            isSelectedSessionExpanded: false
        ) == .seconds(15))
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

    @Test("requirement surveys refresh from relevant events with a low-frequency fallback")
    func requirementSurveyMonitoringUsesEventStreams() {
        #expect(RequirementSurveyMonitoringPolicy.consistencyCheckInterval == .seconds(300))
        #expect(RequirementSurveyMonitoringPolicy.shouldRefresh(forEventTypes: [
            "user_input_requested",
        ]))
        #expect(RequirementSurveyMonitoringPolicy.shouldRefresh(forEventTypes: [
            "requirement_survey_resolved",
        ]))
        #expect(RequirementSurveyMonitoringPolicy.shouldRefresh(forEventTypes: [
            "run_cancelled",
        ]))
        #expect(!RequirementSurveyMonitoringPolicy.shouldRefresh(forEventTypes: [
            "continuation_requested",
            "run_succeeded",
        ]))
    }

    @Test("notepad refreshes from mutations and Agent tool events with a low-frequency fallback")
    func notepadMonitoringUsesEventStreams() {
        #expect(NotepadExternalSyncPolicy.consistencyCheckInterval == .seconds(300))
        #expect(NotepadExternalSyncPolicy.shouldRefresh(forEventTypes: [
            "tool_batch_completed",
        ]))
        #expect(!NotepadExternalSyncPolicy.shouldRefresh(forEventTypes: [
            "conversation_turn_started",
            "run_succeeded",
        ]))
    }

    @Test("pet consistency refresh exists only while work is active")
    func petStatusRefreshStopsWhenIdle() {
        #expect(!PetStatusRefreshPolicy.shouldRun(isMonitoring: false, activeWorkCount: 1))
        #expect(!PetStatusRefreshPolicy.shouldRun(isMonitoring: true, activeWorkCount: 0))
        #expect(PetStatusRefreshPolicy.shouldRun(isMonitoring: true, activeWorkCount: 1))
        #expect(PetStatusRefreshPolicy.activeInterval == .seconds(20))
    }

    @Test("agent runtime reconciles communications quickly while idle heartbeat polling backs off")
    func agentRuntimeRecoveryAndHeartbeatPollingIntervals() {
        #expect(AgentRuntimePollingPolicy.communicationRecoveryInterval == .seconds(30))
        #expect(AgentRuntimePollingPolicy.shouldWakeCommunicationRecovery(for: .roomUpdated))
        #expect(!AgentRuntimePollingPolicy.shouldWakeCommunicationRecovery(for: .runUpdated))
        #expect(!AgentRuntimePollingPolicy.shouldWakeCommunicationRecovery(for: .deliveryClaimed))
        #expect(AgentRuntimePollingPolicy.shouldWakeExecutorRecovery(for: .roomUpdated))
        #expect(!AgentRuntimePollingPolicy.shouldWakeExecutorRecovery(for: .runUpdated))
        #expect(!AgentRuntimePollingPolicy.shouldWakeExecutorRecovery(for: .deliveryClaimed))
        #expect(AgentRuntimePollingPolicy.heartbeatDelayMilliseconds(
            nextDueUnixMs: nil,
            nowUnixMs: 1_000
        ) == 1_800_000)
        #expect(AgentRuntimePollingPolicy.heartbeatDelayMilliseconds(
            nextDueUnixMs: 11_000,
            nowUnixMs: 1_000
        ) == 10_000)
        #expect(AgentRuntimePollingPolicy.heartbeatDelayMilliseconds(
            nextDueUnixMs: 1_000_000,
            nowUnixMs: 1_000
        ) == 300_000)
    }

    @Test("agent artifact storage wakes from changes and sleeps longer without retries")
    func agentArtifactStorageUsesChangesAndDueTime() {
        #expect(AgentRuntimePollingPolicy.shouldWakeArtifactStorage(for: .roomUpdated))
        #expect(!AgentRuntimePollingPolicy.shouldWakeArtifactStorage(for: .runUpdated))
        #expect(!AgentRuntimePollingPolicy.shouldWakeArtifactStorage(for: .deliveryClaimed))
        #expect(AgentRuntimePollingPolicy.artifactStorageDelayMilliseconds(
            nextDueUnixMs: 900,
            nowUnixMs: 1_000
        ) == 1_000)
        #expect(AgentRuntimePollingPolicy.artifactStorageDelayMilliseconds(
            nextDueUnixMs: 11_000,
            nowUnixMs: 1_000
        ) == 10_000)
        #expect(AgentRuntimePollingPolicy.artifactStorageDelayMilliseconds(
            nextDueUnixMs: nil,
            nowUnixMs: 1_000
        ) == 1_800_000)
    }

    @Test("direct Agent chat reloads only for timeline mutations")
    func directAgentChatIgnoresRunCheckpoints() {
        #expect(AgentDirectChatRefreshPolicy.shouldReloadTimeline(for: .roomUpdated))
        #expect(!AgentDirectChatRefreshPolicy.shouldReloadTimeline(for: .runUpdated))
        #expect(!AgentDirectChatRefreshPolicy.shouldReloadTimeline(for: .deliveryClaimed))
    }

    @Test("pet task process uses activity versions instead of frequent fallback polling")
    func petTaskProcessUsesRealtimeActivityVersions() {
        #expect(PetTaskProcessRefreshPolicy.fallbackInterval(
            hasRealtimeIdentity: true
        ) == .seconds(60))
        #expect(PetTaskProcessRefreshPolicy.fallbackInterval(
            hasRealtimeIdentity: false
        ) == .seconds(5))
    }

    @Test("task reply inspector uses realtime as its primary refresh path")
    func taskReplyInspectorPollingBacksOffWithRealtime() {
        #expect(TaskReplyInspectorPollingPolicy.interval(
            hasActiveRealtimeStream: true
        ) == .seconds(60))
        #expect(TaskReplyInspectorPollingPolicy.interval(
            hasActiveRealtimeStream: false
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

    @Test("clipboard ignores high-confidence credential payloads")
    func clipboardSensitiveContentIsIgnored() {
        #expect(ClipboardSensitiveContentPolicy.shouldIgnore("""
            {"WARNING_BANNER":"DO NOT SHARE this credential", "accessToken":"secret"}
            """))
        #expect(ClipboardSensitiveContentPolicy.shouldIgnore("""
            {"access_token":"secret", "refresh_token":"secret"}
            """))
        #expect(ClipboardSensitiveContentPolicy.shouldIgnore("""
            -----BEGIN OPENSSH PRIVATE KEY-----
            secret
            """))
        #expect(!ClipboardSensitiveContentPolicy.shouldIgnore("""
            Documentation mentioning accessToken without containing a credential payload.
            """))
    }

    @Test("metadata file search has a bounded timeout and result count")
    func metadataFileSearchIsBounded() {
        #expect(MetadataFileSearchPolicy.timeout == .seconds(3))
        #expect(MetadataFileSearchPolicy.maximumResultCount == 50)
    }

    @Test("clipboard thumbnails bound decoded pixels and retained images")
    func clipboardThumbnailsAreBounded() {
        #expect(ClipboardThumbnailPolicy.maximumSourcePixelCount == 64_000_000)
        #expect(ClipboardThumbnailPolicy.maximumDisplayPixelSize == 180)
        #expect(ClipboardThumbnailPolicy.maximumCachedCount == 64)
    }

    @Test("screenshot pasteboard payload is bounded")
    func screenshotPasteboardPayloadIsBounded() {
        #expect(ScreenshotPersistencePolicy.maximumPasteboardBytes == 128 * 1_024 * 1_024)
        #expect(ScreenshotPersistencePolicy.maximumTranslationBytes == 20 * 1_024 * 1_024)
    }

    @Test("media studio reference images are bounded before upload")
    func mediaStudioReferenceImagesAreBounded() {
        #expect(MediaStudioInputImagePolicy.maximumBytes == 20 * 1_024 * 1_024)
        #expect(MediaStudioInputImagePolicy.maximumSourcePixelCount == 64_000_000)
    }

    @Test("task graph polling uses realtime as the primary update path")
    func taskGraphPollingBacksOffWithRealtime() {
        #expect(MessageTaskPollingPolicy.interval(
            isEmptyGraphRetry: true,
            hasActiveRealtimeStream: true
        ) == .milliseconds(600))
        #expect(MessageTaskPollingPolicy.interval(
            isEmptyGraphRetry: false,
            hasActiveRealtimeStream: false
        ) == .seconds(2))
        #expect(MessageTaskPollingPolicy.interval(
            isEmptyGraphRetry: false,
            hasActiveRealtimeStream: true
        ) == .seconds(60))
    }

    @Test("application activation keeps an existing artifact storage coordinator")
    func applicationActivationKeepsArtifactStorageCoordinator() {
        #expect(!AgentArtifactStorageCoordinatorPolicy.shouldStart(
            existingOwnerUserID: "owner",
            requestedOwnerUserID: "owner",
            hasLiveTask: true,
            forceRestart: false
        ))
        #expect(AgentArtifactStorageCoordinatorPolicy.shouldStart(
            existingOwnerUserID: "old-owner",
            requestedOwnerUserID: "new-owner",
            hasLiveTask: true,
            forceRestart: false
        ))
        #expect(AgentArtifactStorageCoordinatorPolicy.shouldStart(
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

    @Test("collapsed project directories evict only their cached subtree")
    func collapsedProjectDirectoriesEvictTheirSubtree() {
        let cached = [
            "/project",
            "/project/Sources",
            "/project/Sources/Feature",
            "/project/Tests",
            "/project/SourcesElsewhere",
        ]
        let evicted = cached.filter {
            ProjectDirectoryMemoryPolicy.isCachedPath(
                $0,
                insideSubtree: "/project/Sources"
            )
        }

        #expect(evicted == ["/project/Sources", "/project/Sources/Feature"])
    }

    @Test("project code navigation history remains bounded")
    func projectCodeNavigationHistoryRemainsBounded() {
        var history = Array(0..<200)
        ProjectDirectoryMemoryPolicy.trimNavigationHistory(&history)

        #expect(history.count == ProjectDirectoryMemoryPolicy.maximumNavigationHistory)
        #expect(history.first == 72)
        #expect(history.last == 199)
    }

    @Test("project directory expansion preferences are bounded and root scoped")
    func projectDirectoryExpansionPreferencesAreBounded() {
        let root = "/project"
        var paths = Set((0..<511).map { "\(root)/folder-\($0)" })
        paths.formUnion((0..<20).map { "\(root)/folder-0/deep-\($0)" })
        paths.insert(root)
        paths.insert("/another-project/folder")

        let normalized = ProjectDirectoryExpansionStatePolicy.normalizedPaths(
            paths,
            rootPath: root
        )

        #expect(normalized.count == ProjectDirectoryExpansionStatePolicy.maximumPersistedPaths)
        #expect(!normalized.contains(root))
        #expect(!normalized.contains("/another-project/folder"))
        #expect((0..<511).allSatisfy { normalized.contains("\(root)/folder-\($0)") })

        let encodedRoot = "local://connector/device/workspace/Library/Application%20Support/%E9%A1%B9%E7%9B%AE"
        let decodedChild = "local://connector/device/workspace/Library/Application Support/项目/docs"
        #expect(ProjectDirectoryExpansionStatePolicy.normalizedPaths(
            [decodedChild],
            rootPath: encodedRoot
        ) == [decodedChild])
    }

    @Test("complete directory listings remove only missing direct expanded subtrees")
    func projectDirectoryExpansionRemovesStaleSubtrees() {
        let expanded: Set<String> = [
            "/project/kept",
            "/project/missing",
            "/project/missing/child",
            "/project/kept/deep",
        ]
        let stale = ProjectDirectoryMemoryPolicy.staleExpandedSubtreeRoots(
            expandedPaths: expanded,
            parentPath: "/project",
            availableDirectoryPaths: ["/project/kept"],
            isListingTruncated: false
        )

        #expect(stale == ["/project/missing"])
        #expect(ProjectDirectoryMemoryPolicy.staleExpandedSubtreeRoots(
            expandedPaths: expanded,
            parentPath: "/project",
            availableDirectoryPaths: ["/project/kept"],
            isListingTruncated: true
        ).isEmpty)
    }

    @Test("agent attachment cache keeps only newest images within its byte budget")
    func agentAttachmentCacheIsBoundedAndRecent() {
        func attachment(
            _ id: String,
            size: Int,
            kind: ConversationAttachmentKind = .image
        ) -> ProjectAgentMessageAttachment {
            .init(
                id: id,
                name: "\(id).png",
                mimeType: kind == .image ? "image/png" : "text/plain",
                size: size,
                kind: kind,
                origin: .file
            )
        }

        func message(
            _ id: String,
            createdAt: Int64,
            attachments: [ProjectAgentMessageAttachment]
        ) -> ProjectAgentMessage {
            .init(
                id: id,
                ownerUserID: "owner",
                roomID: "room",
                draft: .init(senderKind: .human, senderID: "owner", content: id),
                rootMessageID: id,
                attachments: attachments,
                createdAtUnixMs: createdAt
            )
        }

        let messages = [
            message("old", createdAt: 1, attachments: [attachment("old-image", size: 4)]),
            message("new", createdAt: 2, attachments: [
                attachment("new-large", size: 7),
                attachment("new-file", size: 1, kind: .file),
                attachment("new-small", size: 3),
            ]),
        ]
        let plan = AgentAttachmentDataCachePolicy.loadPlan(
            messages: messages,
            maximumBytes: 10,
            maximumCount: 2
        )

        #expect(plan.map(\.attachment.id) == ["new-large", "new-small"])

        let retained = AgentAttachmentDataCachePolicy.retainedData([
            "old-image": Data(repeating: 1, count: 4),
            "new-large": Data(repeating: 2, count: 7),
        ], for: plan)
        #expect(Set(retained.keys) == ["new-large"])
        #expect(AgentAttachmentDataCachePolicy.missingRequests(
            in: plan,
            cachedDataByID: retained
        ).map(\.attachment.id) == ["new-small"])
    }
}
