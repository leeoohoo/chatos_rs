@testable import ChatOSApp
import ChatOSCore
import Foundation
import XCTest

@MainActor
final class RemoteTerminalViewModelTests: XCTestCase {
    func testVerificationChallengeReconnectsWithSubmittedCode() async {
        let session = RemoteTerminalSessionTestDouble()
        let provider = RemoteTerminalProviderTestDouble(
            sessions: [session],
            requiresVerificationCode: true
        )
        let model = makeModel(provider: provider)

        model.startIfNeeded()
        await waitUntil { model.verificationPrompt != nil }
        XCTAssertEqual(model.state, .idle)

        model.verificationCode = "123456"
        model.submitVerificationCode()
        await waitUntil { model.state == .running }

        let requests = await provider.requests
        XCTAssertEqual(requests.map(\.verificationCode), [nil, "123456"])
        XCTAssertTrue(session.didStart)
    }

    func testReconnectRejectsEventsFromPreviousSession() async {
        let oldSession = RemoteTerminalSessionTestDouble(closeFinishesEvents: false)
        let currentSession = RemoteTerminalSessionTestDouble()
        let provider = RemoteTerminalProviderTestDouble(sessions: [oldSession, currentSession])
        let model = makeModel(provider: provider)

        model.startIfNeeded()
        await waitUntil { model.state == .running }
        model.reconnect()
        await waitUntil { await provider.requestCount == 2 && model.state == .running }

        oldSession.emit(.terminated(exitCode: 9))
        await Task.yield()
        XCTAssertEqual(model.state, .running)

        currentSession.emit(.terminated(exitCode: 0))
        await waitUntil { model.state == .exited(0) }
    }

    func testViewModelCanReleaseWhileProviderIgnoresCancellation() async {
        let session = RemoteTerminalSessionTestDouble()
        let provider = RemoteTerminalProviderTestDouble(
            sessions: [session],
            suspendFirstRequest: true
        )
        var model: NativeRemoteTerminalViewModel? = makeModel(provider: provider)
        weak var weakModel: NativeRemoteTerminalViewModel?
        weakModel = model

        model?.startIfNeeded()
        await provider.waitForFirstRequest()
        model = nil

        XCTAssertNil(weakModel)
        await provider.resumeFirstRequest()
        await waitUntil { session.didClose }
    }

    private func makeModel(
        provider: RemoteTerminalProviderTestDouble
    ) -> NativeRemoteTerminalViewModel {
        NativeRemoteTerminalViewModel(
            connectionID: "connection-1",
            connectionName: "Server",
            initialWorkingDirectory: "~",
            sessionProvider: provider
        )
    }

    private func waitUntil(
        _ predicate: @escaping @MainActor () async -> Bool
    ) async {
        for _ in 0..<1_000 {
            if await predicate() { return }
            await Task.yield()
        }
        XCTFail("condition was not met")
    }
}

private struct RemoteTerminalProviderTestRequest: Sendable, Equatable {
    let connectionID: String
    let verificationCode: String?
}

private actor RemoteTerminalProviderTestDouble: RemoteTerminalSessionProviding {
    private var sessions: [RemoteTerminalSessionTestDouble]
    private let requiresVerificationCode: Bool
    private let suspendFirstRequest: Bool
    private var firstRequestStarted = false
    private var firstRequestContinuation: CheckedContinuation<Void, Never>?
    private(set) var requests: [RemoteTerminalProviderTestRequest] = []

    init(
        sessions: [RemoteTerminalSessionTestDouble],
        requiresVerificationCode: Bool = false,
        suspendFirstRequest: Bool = false
    ) {
        self.sessions = sessions
        self.requiresVerificationCode = requiresVerificationCode
        self.suspendFirstRequest = suspendFirstRequest
    }

    var requestCount: Int { requests.count }

    func openRemoteTerminalSession(
        connectionID: String,
        verificationCode: String?
    ) async throws -> any RemoteTerminalSessionPort {
        requests.append(.init(
            connectionID: connectionID,
            verificationCode: verificationCode
        ))
        if suspendFirstRequest, requests.count == 1 {
            firstRequestStarted = true
            await withCheckedContinuation { firstRequestContinuation = $0 }
        }
        if requiresVerificationCode, verificationCode == nil {
            throw RemoteVerificationChallenge(prompt: "OTP")
        }
        return sessions.removeFirst()
    }

    func waitForFirstRequest() async {
        while !firstRequestStarted { await Task.yield() }
    }

    func resumeFirstRequest() {
        firstRequestContinuation?.resume()
        firstRequestContinuation = nil
    }
}

private final class RemoteTerminalSessionTestDouble: RemoteTerminalSessionPort,
    @unchecked Sendable {
    private let stream: AsyncStream<RemoteTerminalEvent>
    private let continuation: AsyncStream<RemoteTerminalEvent>.Continuation
    private let closeFinishesEvents: Bool
    private(set) var didStart = false
    private(set) var didClose = false

    init(closeFinishesEvents: Bool = true) {
        let pair = AsyncStream<RemoteTerminalEvent>.makeStream()
        self.stream = pair.stream
        self.continuation = pair.continuation
        self.closeFinishesEvents = closeFinishesEvents
    }

    var isRunning: Bool { didStart && !didClose }
    func events() -> AsyncStream<RemoteTerminalEvent> { stream }
    func start(columns: Int, rows: Int) { didStart = true }
    func send(_ data: Data) {}
    func resize(columns: Int, rows: Int) {}
    func interrupt() {}

    func close() {
        didClose = true
        if closeFinishesEvents { continuation.finish() }
    }

    func emit(_ event: RemoteTerminalEvent) {
        continuation.yield(event)
    }
}
