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
        let mainActorMaximumGapMilliseconds: Double
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

        let probe = AppScenarioMainActorGapProbe()
        let probeTask = Task { await probe.run() }
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

        let timelineStart = DispatchTime.now().uptimeNanoseconds
        let timelineItems = ConversationTimelineItem.build(
            turns: turns,
            promptsByTurnID: [:],
            unattachedPrompts: []
        )
        let timelineMilliseconds = Self.milliseconds(since: timelineStart)

        let todoStart = DispatchTime.now().uptimeNanoseconds
        let todoPresentations = TeamTodoRunPresentation.presentationsByTodoID(
            summaries: todoSummaries
        )
        let todoMilliseconds = Self.milliseconds(since: todoStart)

        let nativeTimeline = try await measureNativeTimeline(rows: 1_000)

        let (markdownBlockCount, markdownMilliseconds) = await markdownTask.value
        let (codeSpanCount, codeMilliseconds) = await codeTask.value
        let wallMilliseconds = Self.milliseconds(since: wallStart)
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
        XCTAssertLessThan(probe.maximumGapMilliseconds, 100)
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
            mainActorMaximumGapMilliseconds: probe.maximumGapMilliseconds,
            concurrentWallMilliseconds: wallMilliseconds
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
        let encoded = try encoder.encode(measurements)
        let payload = String(decoding: encoded, as: UTF8.self)
        print("CHATOS_APP_SCENARIO_BASELINE \(payload)")
#endif
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

    private func measureNativeTimeline(rows: Int) async throws -> (
        rows: Int,
        initialConstructedRows: Int,
        scrolledConstructedRows: Int,
        initialLayoutMilliseconds: Double,
        scrollLayoutMilliseconds: Double
    ) {
        let coordinator = AgentChatTimelineCoordinator()
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 900, height: 600),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.isReleasedWhenClosed = false
        let scrollView = coordinator.makeScrollView()
        scrollView.frame = window.contentView?.bounds
            ?? NSRect(x: 0, y: 0, width: 900, height: 600)
        window.contentView = scrollView
        defer {
            coordinator.stop()
            window.close()
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
}

@MainActor
private final class AppScenarioMainActorGapProbe {
    private(set) var maximumGapMilliseconds = 0.0
    private(set) var sampleCount = 0

    func run() async {
        var previous = DispatchTime.now().uptimeNanoseconds
        while !Task.isCancelled {
            do {
                try await Task.sleep(for: .milliseconds(1))
            } catch {
                return
            }
            let current = DispatchTime.now().uptimeNanoseconds
            maximumGapMilliseconds = max(
                maximumGapMilliseconds,
                Double(current - previous) / 1_000_000
            )
            sampleCount += 1
            previous = current
        }
    }
}
