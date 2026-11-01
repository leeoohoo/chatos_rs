import AppKit
import ChatOSAgentRuntime
import ChatOSConnector
import ChatOSCore
import Foundation
import SwiftUI
import XCTest
@testable import ChatOSApp

/// Opt-in Release acceptance for the CPU-side work behind the fixed App interaction matrix.
/// The fixture is entirely synthetic and never opens an account or production database.
@MainActor
final class AppPerformanceScenarioBaselineTests: XCTestCase {
    private struct Measurements: Codable {
        let buildConfiguration: String
        let fixtureTurns: Int
        let fixtureTodos: Int
        let markdownBytes: Int
        let codeUTF16Count: Int
        let timelineItemCount: Int
        let nativeTimelineRows: Int
        let nativeTimelineInitialConstructedRows: Int
        let nativeTimelineScrolledConstructedRows: Int
        let timeline1000BuildMilliseconds: Double
        let nativeTimelineInitialLayoutMilliseconds: Double
        let nativeTimelineScrollLayoutMilliseconds: Double
        let todo100ProjectionMilliseconds: Double
        let megabyteMarkdownParseMilliseconds: Double
        let megabyteMarkdownBlockCount: Int
        let largeCodeFallbackMilliseconds: Double
        let largeCodeSpanCount: Int
        let navigationTransitions: Int
        let accountCycles: Int
        let navigationWarmP95Milliseconds: Double
        let navigationMaximumMilliseconds: Double
        let accountCycleP95Milliseconds: Double
        let accountCycleMaximumMilliseconds: Double
        let mainActorMaximumGapMilliseconds: Double
        let mainActorMaximumGapPhase: String
        let concurrentWallMilliseconds: Double
    }

    func testFixedCompositeScenario() async throws {
        guard ProcessInfo.processInfo.environment["CHATOS_RUN_APP_SCENARIO_BASELINE"] == "1" else {
            throw XCTSkip("Set CHATOS_RUN_APP_SCENARIO_BASELINE=1 and run the Release test explicitly.")
        }
#if DEBUG
        throw XCTSkip("The fixed App scenario is a Release-only performance acceptance.")
#else
        let turns = makeTurns(count: 1_000)
        let todoSummaries = makeTodoSummaries(count: 100)
        let markdownRow = "| column | value |\n| --- | --- |\n| key | a moderately long value |\n\n"
        let markdown = "# Large document\n\n" + String(repeating: markdownRow, count: 16_000)
        let code = String(repeating: "let value = 42 // large source\n", count: 20_000)
        XCTAssertGreaterThan(markdown.utf8.count, 1_000_000)
        XCTAssertGreaterThan(code.utf16.count, 600_000)

        // A real chat-page transition reuses the application's existing top-level window.
        // Construct the synthetic XCTest window before sampling so the gate still covers
        // ChatOS scroll/table creation and layout, but not AppKit's unrelated process-wide
        // first-NSWindow initialization.
        let timelineWindow = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 600),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        timelineWindow.isReleasedWhenClosed = false
        defer { timelineWindow.close() }

        let probe = AppScenarioMainActorGapProbe()
        let probeTask = Task.detached(priority: .userInitiated) { [probe] in
            while !Task.isCancelled {
                do {
                    try await Task.sleep(for: .milliseconds(1))
                } catch {
                    return
                }
                let enqueuedAt = DispatchTime.now().uptimeNanoseconds
                await probe.record(enqueuedAt: enqueuedAt)
            }
        }
        await Task.yield()
        let wallStart = DispatchTime.now().uptimeNanoseconds

        let markdownTask = Task.detached(priority: .userInitiated) {
            let cache = MarkdownRenderCache(
                totalCostLimit: 4 * 1_024 * 1_024,
                countLimit: 8
            )
            let start = DispatchTime.now().uptimeNanoseconds
            let blocks = cache.blocks(for: markdown)
            return (blocks.count, Self.milliseconds(since: start))
        }
        let codeTask = Task.detached(priority: .userInitiated) {
            let start = DispatchTime.now().uptimeNanoseconds
            let spans = CodeSyntaxHighlighter.spans(in: code, fileName: "Large.swift")
            return (spans.count, Self.milliseconds(since: start))
        }

        probe.phase = "timeline_projection"
        let timelineStart = DispatchTime.now().uptimeNanoseconds
        let timelineItems = ConversationTimelineItem.build(
            turns: turns,
            promptsByTurnID: [:],
            unattachedPrompts: []
        )
        let timelineMilliseconds = Self.milliseconds(since: timelineStart)

        probe.phase = "todo_projection"
        let todoStart = DispatchTime.now().uptimeNanoseconds
        let todoPresentations = TeamTodoRunPresentation.presentationsByTodoID(
            summaries: todoSummaries
        )
        let todoMilliseconds = Self.milliseconds(since: todoStart)

        probe.phase = "native_timeline"
        let nativeTimeline = try await measureNativeTimeline(
            rows: 1_000,
            window: timelineWindow
        )

        probe.phase = "background_results"
        let (markdownBlockCount, markdownMilliseconds) = await markdownTask.value
        let (codeSpanCount, codeMilliseconds) = await codeTask.value
        let wallMilliseconds = Self.milliseconds(since: wallStart)
        probe.phase = "navigation_account_lifecycle"
        let lifecycle = try await measureNavigationAndAccountLifecycle()
        probeTask.cancel()
        await probeTask.value

        XCTAssertEqual(timelineItems.count, 2_000)
        XCTAssertEqual(todoPresentations.count, 100)
        XCTAssertEqual(nativeTimeline.rows, 1_000)
        XCTAssertGreaterThan(nativeTimeline.initialConstructedRows, 0)
        XCTAssertLessThan(nativeTimeline.initialConstructedRows, 30)
        XCTAssertGreaterThan(
            nativeTimeline.scrolledConstructedRows,
            nativeTimeline.initialConstructedRows
        )
        XCTAssertLessThan(nativeTimeline.scrolledConstructedRows, 60)
        XCTAssertFalse(markdownBlockCount == 0)
        XCTAssertEqual(codeSpanCount, 0, "600k source must use responsive plain-text fallback")
        XCTAssertGreaterThan(probe.sampleCount, 0)

        // These are generous regression ceilings, not claims about typical latency. The
        // multi-round runner preserves raw samples and applies the tighter comparison gate.
        XCTAssertLessThan(timelineMilliseconds, 100)
        XCTAssertLessThan(nativeTimeline.initialLayoutMilliseconds, 100)
        XCTAssertLessThan(nativeTimeline.scrollLayoutMilliseconds, 100)
        XCTAssertLessThan(todoMilliseconds, 100)
        XCTAssertLessThan(codeMilliseconds, 100)
        XCTAssertLessThan(markdownMilliseconds, 2_000)
        XCTAssertEqual(lifecycle.navigationTransitions, 200)
        XCTAssertEqual(lifecycle.accountCycles, 20)
        XCTAssertLessThan(lifecycle.navigationP95Milliseconds, 100)
        XCTAssertLessThan(lifecycle.navigationMaximumMilliseconds, 250)
        XCTAssertLessThan(probe.maximumGapMilliseconds, 250)
        XCTAssertLessThan(wallMilliseconds, 2_500)

        let measurements = Measurements(
            buildConfiguration: "release",
            fixtureTurns: turns.count,
            fixtureTodos: todoSummaries.count,
            markdownBytes: markdown.utf8.count,
            codeUTF16Count: code.utf16.count,
            timelineItemCount: timelineItems.count,
            nativeTimelineRows: nativeTimeline.rows,
            nativeTimelineInitialConstructedRows: nativeTimeline.initialConstructedRows,
            nativeTimelineScrolledConstructedRows: nativeTimeline.scrolledConstructedRows,
            timeline1000BuildMilliseconds: timelineMilliseconds,
            nativeTimelineInitialLayoutMilliseconds: nativeTimeline.initialLayoutMilliseconds,
            nativeTimelineScrollLayoutMilliseconds: nativeTimeline.scrollLayoutMilliseconds,
            todo100ProjectionMilliseconds: todoMilliseconds,
            megabyteMarkdownParseMilliseconds: markdownMilliseconds,
            megabyteMarkdownBlockCount: markdownBlockCount,
            largeCodeFallbackMilliseconds: codeMilliseconds,
            largeCodeSpanCount: codeSpanCount,
            navigationTransitions: lifecycle.navigationTransitions,
            accountCycles: lifecycle.accountCycles,
            navigationWarmP95Milliseconds: lifecycle.navigationP95Milliseconds,
            navigationMaximumMilliseconds: lifecycle.navigationMaximumMilliseconds,
            accountCycleP95Milliseconds: lifecycle.accountCycleP95Milliseconds,
            accountCycleMaximumMilliseconds: lifecycle.accountCycleMaximumMilliseconds,
            mainActorMaximumGapMilliseconds: probe.maximumGapMilliseconds,
            mainActorMaximumGapPhase: probe.maximumGapPhase,
            concurrentWallMilliseconds: wallMilliseconds
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let encoded = try encoder.encode(measurements)
        let payload = String(decoding: encoded, as: UTF8.self)
        print("CHATOS_APP_SCENARIO_BASELINE \(payload)")
#endif
    }

    private func measureNavigationAndAccountLifecycle() async throws -> (
        navigationTransitions: Int,
        accountCycles: Int,
        navigationP95Milliseconds: Double,
        navigationMaximumMilliseconds: Double,
        accountCycleP95Milliseconds: Double,
        accountCycleMaximumMilliseconds: Double
    ) {
        let service = AppScenarioAuthenticationService()
        let model = AppModel(authenticationService: service)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1_200, height: 800),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(rootView: ChatOSMainSceneView(model: model))
        defer {
            model.prepareForApplicationTermination()
            window.close()
        }

        try await waitUntil("initial signed-out state") {
            model.authentication.phase == .signedOut
        }
        let selections: [SidebarSelection] = [
            .applications,
            .mediaStudio,
            .agentGroupChat,
            .requirementSurveys,
            .terminal("terminal-local"),
        ]
        var navigationSamples: [Double] = []
        var navigationSamplesByDestination: [String: [Double]] = [:]
        var accountCycleSamples: [Double] = []
        var loginSamples: [Double] = []
        var logoutSynchronousSamples: [Double] = []
        var agentWorkspaceIdentities = Set<ObjectIdentifier>()
        var retainedAgentWorkspaces: [AgentGroupChatWorkspaceViewModel] = []

        for cycle in 0..<20 {
            model.authentication.username = "performance-\(cycle % 2)@example.test"
            model.authentication.password = "isolated-performance-password"
            let loginStart = DispatchTime.now().uptimeNanoseconds
            model.authentication.login()
            try await waitUntil("account login \(cycle + 1)") {
                guard case let .authenticated(session) = model.authentication.phase else {
                    return false
                }
                return model.authenticatedUserID == session.user.id
            }
            let loginMilliseconds = Self.milliseconds(since: loginStart)
            loginSamples.append(loginMilliseconds)
            let ownerUserID = try XCTUnwrap(model.authenticatedUserID)
            let agentWorkspace = model.agentWorkspaceViewModel(ownerUserID: ownerUserID)
            XCTAssertTrue(
                agentWorkspace === model.agentWorkspaceViewModel(ownerUserID: ownerUserID),
                "The active account must reuse one Agent workspace model"
            )
            XCTAssertTrue(
                agentWorkspaceIdentities.insert(ObjectIdentifier(agentWorkspace)).inserted,
                "A signed-out account must not retain its prior Agent workspace model"
            )
            retainedAgentWorkspaces.append(agentWorkspace)

            if cycle == 0 {
                for selection in selections {
                    model.selection = selection
                    for _ in 0..<3 { try await settle(window) }
                    if selection == .agentGroupChat,
                       let ownerUserID = model.authenticatedUserID {
                        let agentWorkspace = model.agentWorkspaceViewModel(
                            ownerUserID: ownerUserID
                        )
                        try await waitUntil("Agent workspace warmup") {
                            agentWorkspace.hasLoadedInitialSnapshot
                        }
                    }
                }
                for offset in 0..<200 {
                    let started = DispatchTime.now().uptimeNanoseconds
                    let selection = selections[offset % selections.count]
                    model.selection = selection
                    try await settle(window)
                    let elapsed = Self.milliseconds(since: started)
                    navigationSamples.append(elapsed)
                    navigationSamplesByDestination[Self.navigationName(selection), default: []]
                        .append(elapsed)
                }
            }

            let logoutStart = DispatchTime.now().uptimeNanoseconds
            model.authentication.logout()
            logoutSynchronousSamples.append(Self.milliseconds(since: logoutStart))
            try await waitUntil("account logout \(cycle + 1)") {
                model.authentication.phase == .signedOut && model.authenticatedUserID == nil
            }
            XCTAssertTrue(model.agentWorkspaceViewModels.isEmpty)
            let logoutMilliseconds = Self.milliseconds(since: logoutStart)
            accountCycleSamples.append(loginMilliseconds + logoutMilliseconds)
        }
        try await waitUntil("authentication service logout completion") {
            await service.operationCounts() == (restore: 1, login: 20, logout: 20)
        }

        let destinationP95 = navigationSamplesByDestination.mapValues {
            Self.nearestRank($0, percentile: 0.95)
        }
        let diagnostics: [String: Any] = [
            "navigation_p95_by_destination_ms": destinationP95,
            "login_p95_ms": Self.nearestRank(loginSamples, percentile: 0.95),
            "login_max_ms": loginSamples.max() ?? .infinity,
            "logout_synchronous_p95_ms": Self.nearestRank(
                logoutSynchronousSamples,
                percentile: 0.95
            ),
            "logout_synchronous_max_ms": logoutSynchronousSamples.max() ?? .infinity,
        ]
        let diagnosticsData = try JSONSerialization.data(
            withJSONObject: diagnostics,
            options: [.sortedKeys]
        )
        print(
            "CHATOS_APP_LIFECYCLE_DIAGNOSTICS "
                + String(decoding: diagnosticsData, as: UTF8.self)
        )

        return (
            navigationTransitions: navigationSamples.count,
            accountCycles: accountCycleSamples.count,
            navigationP95Milliseconds: Self.nearestRank(navigationSamples, percentile: 0.95),
            navigationMaximumMilliseconds: navigationSamples.max() ?? .infinity,
            accountCycleP95Milliseconds: Self.nearestRank(accountCycleSamples, percentile: 0.95),
            accountCycleMaximumMilliseconds: accountCycleSamples.max() ?? .infinity
        )
    }

    private func settle(_ window: NSWindow) async throws {
        window.contentView?.layoutSubtreeIfNeeded()
        await Task.yield()
        try await Task.sleep(for: .milliseconds(1))
        window.contentView?.layoutSubtreeIfNeeded()
    }

    private func waitUntil(
        _ operation: String,
        timeout: Duration = .seconds(5),
        predicate: @escaping @MainActor () async -> Bool
    ) async throws {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: timeout)
        while clock.now < deadline {
            if await predicate() { return }
            try await Task.sleep(for: .milliseconds(1))
        }
        XCTFail("Timed out waiting for \(operation)")
        throw AppScenarioError.timeout(operation)
    }

    private func makeTurns(count: Int) -> [ConversationTurn] {
        (0..<count).map { index in
            let timestamp = Date(timeIntervalSince1970: Double(index))
            return ConversationTurn(
                id: "turn-\(index)",
                sessionID: "performance-session",
                sequence: Int64(index),
                revision: 1,
                userMessage: .init(
                    id: "user-\(index)",
                    role: .user,
                    text: "User request \(index)",
                    createdAt: timestamp
                ),
                assistantReplies: [.init(message: .init(
                    id: "reply-\(index)",
                    role: .assistant,
                    text: "Assistant reply \(index)",
                    createdAt: timestamp.addingTimeInterval(0.5)
                ))],
                status: .completed,
                startedAt: timestamp,
                completedAt: timestamp.addingTimeInterval(1)
            )
        }
    }

    private func makeTodoSummaries(count: Int) -> [LocalAgentTodoRunSummary] {
        (0..<count).map { index in
            .init(
                todoID: "todo-\(index)",
                runID: UUID(),
                status: .completed,
                receiptCount: 3,
                committedPaths: ["Sources/Feature\(index).swift"],
                updatedAtUnixMs: Int64(index)
            )
        }
    }

    private func measureNativeTimeline(
        rows: Int,
        window: NSWindow
    ) async throws -> (
        rows: Int,
        initialConstructedRows: Int,
        scrolledConstructedRows: Int,
        initialLayoutMilliseconds: Double,
        scrollLayoutMilliseconds: Double
    ) {
        let coordinator = AgentChatTimelineCoordinator()
        let scrollView = coordinator.makeScrollView()
        scrollView.frame = window.contentView?.bounds
            ?? NSRect(x: 0, y: 0, width: 900, height: 600)
        window.contentView = scrollView
        defer {
            coordinator.stop()
        }

        var constructedIDs = Set<String>()
        let entries = (0..<rows).map { index in
            AgentChatTimelineEntry(id: "scenario-row-\(index)", value: index) {
                constructedIDs.insert("scenario-row-\(index)")
                return AnyView(Text("Scenario row \(index)").frame(height: 100))
            }
        }
        let input = AgentChatNativeTimeline(
            entries: entries,
            rowState: .init(),
            fontScale: 1,
            colorScheme: .light,
            isInitialContentReady: true,
            scrollToLatestRequest: 0
        )

        let initialStart = DispatchTime.now().uptimeNanoseconds
        coordinator.update(input)
        try await settleNativeTimeline(coordinator)
        let initialMilliseconds = Self.milliseconds(since: initialStart)
        let initialConstructedRows = constructedIDs.count

        let scrollStart = DispatchTime.now().uptimeNanoseconds
        scrollView.contentView.scroll(to: NSPoint(x: 0, y: 8_000))
        scrollView.reflectScrolledClipView(scrollView.contentView)
        try await settleNativeTimeline(coordinator)
        let scrollMilliseconds = Self.milliseconds(since: scrollStart)

        return (
            rows: coordinator.table.numberOfRows,
            initialConstructedRows: initialConstructedRows,
            scrolledConstructedRows: constructedIDs.count,
            initialLayoutMilliseconds: initialMilliseconds,
            scrollLayoutMilliseconds: scrollMilliseconds
        )
    }

    private func settleNativeTimeline(
        _ coordinator: AgentChatTimelineCoordinator
    ) async throws {
        for _ in 0..<3 {
            coordinator.scrollView?.layoutSubtreeIfNeeded()
            coordinator.table.layoutSubtreeIfNeeded()
            try await Task.sleep(for: .milliseconds(2))
        }
    }

    nonisolated private static func milliseconds(since start: UInt64) -> Double {
        Double(DispatchTime.now().uptimeNanoseconds - start) / 1_000_000
    }

    nonisolated private static func nearestRank(
        _ values: [Double],
        percentile: Double
    ) -> Double {
        let sorted = values.sorted()
        guard !sorted.isEmpty else { return .infinity }
        let index = max(0, Int(ceil(Double(sorted.count) * percentile)) - 1)
        return sorted[index]
    }

    nonisolated private static func navigationName(_ selection: SidebarSelection) -> String {
        switch selection {
        case .applications: "applications"
        case .mediaStudio: "media_studio"
        case .agentGroupChat: "agent"
        case .requirementSurveys: "requirement_surveys"
        case .terminal: "terminal"
        default: "other"
        }
    }
}

private enum AppScenarioError: Error {
    case timeout(String)
}

private actor AppScenarioAuthenticationService: AuthenticationServicing {
    private var restoreCount = 0
    private var loginCount = 0
    private var logoutCount = 0

    func restoreSession() async throws -> AuthSession? {
        restoreCount += 1
        return nil
    }

    func login(username: String, password: String) async throws -> AuthSession {
        loginCount += 1
        return AuthSession(user: .init(
            id: "performance-user-\(username)",
            username: username,
            displayName: "Performance User",
            role: "user"
        ))
    }

    func sendRegistrationCode(
        email: String,
        inviteCode: String
    ) async throws -> RegistrationCodeDelivery {
        .init(expiresInSeconds: 600, resendAfterSeconds: 60)
    }

    func register(
        email: String,
        password: String,
        inviteCode: String,
        verificationCode: String
    ) async throws -> AuthSession {
        try await login(username: email, password: password)
    }

    func logout() async {
        logoutCount += 1
    }

    func operationCounts() -> (restore: Int, login: Int, logout: Int) {
        (restoreCount, loginCount, logoutCount)
    }
}

@MainActor
private final class AppScenarioMainActorGapProbe {
    private(set) var maximumGapMilliseconds = 0.0
    private(set) var maximumGapPhase = "unassigned"
    private(set) var sampleCount = 0
    var phase = "setup"

    func record(enqueuedAt: UInt64) {
        let current = DispatchTime.now().uptimeNanoseconds
        let gap = Double(current - enqueuedAt) / 1_000_000
        if gap > maximumGapMilliseconds {
            maximumGapMilliseconds = gap
            maximumGapPhase = phase
        }
        sampleCount += 1
    }
}
