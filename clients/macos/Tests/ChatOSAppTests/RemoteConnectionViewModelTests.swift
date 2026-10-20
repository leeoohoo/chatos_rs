@testable import ChatOSApp
import ChatOSCore
import Foundation
import XCTest

@MainActor
final class RemoteConnectionViewModelTests: XCTestCase {
    func testEditorSaveUsesApplicationBoundary() async {
        let service = RemoteConnectionViewModelService()
        let model = makeEditor(service: service)

        model.save()
        await waitUntil { model.savedConnection != nil }

        let calls = await service.calls
        XCTAssertEqual(model.savedConnection?.host, "server.example.com")
        XCTAssertFalse(model.isSaving)
        XCTAssertEqual(calls, [.create])
    }

    func testEditorCanReleaseWhileConnectorIgnoresCancellation() async {
        let service = RemoteConnectionViewModelService(suspendDraftTest: true)
        var model: RemoteConnectionEditorViewModel? = makeEditor(service: service)
        weak var weakModel: RemoteConnectionEditorViewModel?
        weakModel = model

        model?.testConnection()
        await service.waitForDraftTest()
        model = nil

        XCTAssertNil(weakModel)
        await service.resumeDraftTest(with: .success(.init(success: true, message: nil)))
    }

    func testDetailDeactivateRejectsLateDelete() async {
        let service = RemoteConnectionViewModelService(suspendDelete: true)
        let model = RemoteConnectionDetailViewModel(
            connectionID: "saved",
            useCase: RemoteConnectionManagementUseCase(service: service)
        )

        model.deleteConnection()
        await service.waitForDelete()
        model.deactivate()
        await service.resumeDelete()
        await Task.yield()

        XCTAssertNil(model.deletedConnectionID)
        XCTAssertFalse(model.isDeleting)
    }

    func testVerificationChallengeIsPresentedAndCanBeRetried() async {
        let service = RemoteConnectionViewModelService(
            draftTestResults: [
                .failure(RemoteVerificationChallenge(prompt: "OTP")),
                .success(.init(success: true, message: "verified")),
            ]
        )
        let model = makeEditor(service: service)

        model.testConnection()
        await waitUntil { model.verificationPrompt != nil }
        XCTAssertEqual(model.verificationPrompt, "OTP")

        model.verificationCode = "123456"
        model.submitVerification()
        await waitUntil { model.successMessage != nil }

        XCTAssertEqual(model.successMessage, "verified")
        XCTAssertNil(model.verificationPrompt)
        XCTAssertFalse(model.isTesting)
    }

    private func makeEditor(
        service: RemoteConnectionViewModelService
    ) -> RemoteConnectionEditorViewModel {
        let model = RemoteConnectionEditorViewModel(
            editingConnection: nil,
            connections: [],
            useCase: RemoteConnectionManagementUseCase(service: service)
        )
        model.name = "Server"
        model.host = "server.example.com"
        model.username = "user"
        model.privateKeyPath = "/key"
        return model
    }

    private func waitUntil(
        _ predicate: @escaping @MainActor () -> Bool
    ) async {
        for _ in 0..<1_000 {
            if predicate() { return }
            await Task.yield()
        }
        XCTFail("condition was not met")
    }
}

private enum RemoteConnectionViewModelCall: Sendable, Equatable {
    case create
    case testDraft(String?)
    case delete(String)
}

private enum RemoteConnectionViewModelError: LocalizedError, Sendable {
    case missingPlan

    var errorDescription: String? { "missing test plan" }
}

private actor RemoteConnectionViewModelService: RemoteConnectionServicing {
    private let suspendDraftTest: Bool
    private let suspendDelete: Bool
    private var draftTestResults: [Result<RemoteConnectionTestResult, Error>]
    private var draftTestStarted = false
    private var draftTestContinuation: CheckedContinuation<RemoteConnectionTestResult, Error>?
    private var deleteStarted = false
    private var deleteContinuation: CheckedContinuation<Void, Never>?
    private(set) var calls: [RemoteConnectionViewModelCall] = []

    init(
        suspendDraftTest: Bool = false,
        suspendDelete: Bool = false,
        draftTestResults: [Result<RemoteConnectionTestResult, Error>] = []
    ) {
        self.suspendDraftTest = suspendDraftTest
        self.suspendDelete = suspendDelete
        self.draftTestResults = draftTestResults
    }

    func listConnections() async throws -> [RemoteConnection] { [] }

    func createConnection(_ draft: RemoteConnectionDraft) async throws -> RemoteConnection {
        calls.append(.create)
        return connection(id: "created", draft: draft)
    }

    func updateConnection(
        id: String,
        draft: RemoteConnectionDraft
    ) async throws -> RemoteConnection {
        connection(id: id, draft: draft)
    }

    func deleteConnection(id: String) async throws {
        calls.append(.delete(id))
        guard suspendDelete else { return }
        deleteStarted = true
        await withCheckedContinuation { deleteContinuation = $0 }
    }

    func testDraft(
        _ draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        calls.append(.testDraft(verificationCode))
        if suspendDraftTest {
            draftTestStarted = true
            return try await withCheckedThrowingContinuation { draftTestContinuation = $0 }
        }
        guard !draftTestResults.isEmpty else {
            return .init(success: true, message: nil)
        }
        return try draftTestResults.removeFirst().get()
    }

    func testSaved(
        id: String,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        .init(success: true, message: nil)
    }

    func waitForDraftTest() async {
        while !draftTestStarted { await Task.yield() }
    }

    func resumeDraftTest(
        with result: Result<RemoteConnectionTestResult, Error>
    ) {
        switch result {
        case let .success(value): draftTestContinuation?.resume(returning: value)
        case let .failure(error): draftTestContinuation?.resume(throwing: error)
        }
        draftTestContinuation = nil
    }

    func waitForDelete() async {
        while !deleteStarted { await Task.yield() }
    }

    func resumeDelete() {
        deleteContinuation?.resume()
        deleteContinuation = nil
    }

    private func connection(
        id: String,
        draft: RemoteConnectionDraft
    ) -> RemoteConnection {
        RemoteConnection(
            id: id,
            name: draft.name ?? draft.host,
            host: draft.host,
            port: draft.port,
            username: draft.username,
            authenticationType: draft.authenticationType,
            hasPassword: draft.password != nil,
            hasPrivateKeyPath: draft.privateKeyPath != nil,
            hasCertificatePath: draft.certificatePath != nil,
            defaultRemotePath: draft.defaultRemotePath,
            hostKeyPolicy: draft.hostKeyPolicy,
            localConnectorDeviceID: "device",
            localConnectorWorkspaceID: "workspace",
            jumpEnabled: false,
            jumpConnectionID: nil,
            jumpHost: nil,
            jumpPort: nil,
            jumpUsername: nil,
            hasJumpPrivateKeyPath: false,
            hasJumpCertificatePath: false,
            hasJumpPassword: false,
            lastActiveAt: nil
        )
    }
}
