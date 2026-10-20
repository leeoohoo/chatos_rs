import ChatOSCore
import Foundation
import Testing
@testable import ChatOSApp

@Suite("Remote SFTP verification")
@MainActor
struct RemoteSFTPViewModelTests {
    @Test("prompts for MFA and loads the remote directory after verification")
    func verifiesBeforeLoading() async {
        let service = RemoteSFTPVerificationServiceStub()
        let viewModel = RemoteSFTPViewModel(connectionID: "server-1", service: service)

        viewModel.load()
        await waitUntil { viewModel.verificationPrompt != nil }

        #expect(viewModel.verificationPrompt == "Please Input Mfa Code (SMS):")
        #expect(viewModel.remoteEntries.isEmpty)

        viewModel.verificationCode = "614207"
        viewModel.submitVerification()
        await waitUntil { viewModel.remotePath == "/srv/app" }

        #expect(viewModel.verificationPrompt == nil)
        #expect(viewModel.errorMessage == nil)
        #expect(viewModel.remotePath == "/srv/app")
        #expect(viewModel.remoteEntries.map(\.name) == ["app.log"])
        #expect(await service.submittedCodes() == [nil, "614207"])
    }

    @Test("newer remote navigation rejects an older listing")
    func latestRemoteNavigationWins() async {
        let service = RemoteSFTPLifecycleServiceStub(suspendFirstListing: true)
        let viewModel = RemoteSFTPViewModel(connectionID: "server-1", service: service)

        viewModel.loadRemote(path: "/old")
        await service.waitForFirstListing()
        viewModel.loadRemote(path: "/new")
        await waitUntil { viewModel.remotePath == "/new" }
        await service.resumeFirstListing()
        await Task.yield()

        #expect(viewModel.remotePath == "/new")
        #expect(!viewModel.isLoadingRemote)
    }

    @Test("workspace can release while an obsolete listing ignores cancellation")
    func workspaceCanReleaseDuringListing() async {
        let service = RemoteSFTPLifecycleServiceStub(suspendFirstListing: true)
        var viewModel: RemoteSFTPViewModel? = RemoteSFTPViewModel(
            connectionID: "server-1",
            service: service
        )
        weak var weakViewModel: RemoteSFTPViewModel?
        weakViewModel = viewModel

        viewModel?.loadRemote(path: "/old")
        await service.waitForFirstListing()
        viewModel?.deactivate()
        viewModel = nil

        #expect(weakViewModel == nil)
        await service.resumeFirstListing()
    }

    @Test("committed delete with refresh failure is not reported as operation failure")
    func committedDeletePreservesSuccess() async {
        let service = RemoteSFTPLifecycleServiceStub(failListingAfterDelete: true)
        let viewModel = RemoteSFTPViewModel(connectionID: "server-1", service: service)
        viewModel.loadRemote(path: "/srv")
        await waitUntil { viewModel.remoteEntries.count == 1 }
        viewModel.selectedRemotePath = "/srv/file.txt"

        viewModel.deleteSelectedRemote()
        await waitUntil { !viewModel.isTransferring && viewModel.notice != nil }

        #expect(await service.deletedPaths == ["/srv/file.txt"])
        #expect(viewModel.notice == "已删除 file.txt")
        #expect(viewModel.refreshWarning != nil)
        #expect(viewModel.errorMessage == nil)
    }

    private func waitUntil(
        _ predicate: @escaping @MainActor () -> Bool
    ) async {
        for _ in 0..<1_000 {
            if predicate() { return }
            await Task.yield()
        }
        Issue.record("condition was not met")
    }
}

private actor RemoteSFTPVerificationServiceStub: RemoteFileServicing {
    private var codes: [String?] = []

    func authenticate(connectionID: String, verificationCode: String?) async throws {
        codes.append(verificationCode)
        guard verificationCode == "614207" else {
            throw RemoteVerificationChallenge(prompt: "Please Input Mfa Code (SMS):")
        }
    }

    func initialDirectory(connectionID: String) async throws -> String { "/srv/app" }

    func listDirectory(connectionID: String, path: String) async throws -> RemoteDirectoryListing {
        .init(
            path: path,
            parentPath: "/srv",
            entries: [
                .init(
                    name: "app.log",
                    path: "/srv/app/app.log",
                    kind: .file,
                    size: 42,
                    modifiedAt: nil,
                    permissions: "-rw-r--r--"
                ),
            ]
        )
    }

    func uploadFile(
        connectionID: String,
        localURL: URL,
        remoteDirectory: String,
        overwrite: Bool
    ) async throws -> String { remoteDirectory }

    func downloadFile(
        connectionID: String,
        remotePath: String,
        localURL: URL,
        overwrite: Bool
    ) async throws {}

    func createDirectory(connectionID: String, parentPath: String, name: String) async throws {}
    func renameEntry(connectionID: String, path: String, newName: String) async throws {}
    func deleteEntry(connectionID: String, path: String, recursively: Bool) async throws {}

    func submittedCodes() -> [String?] { codes }
}

private enum RemoteSFTPLifecycleTestError: Error {
    case refreshFailed
}

private actor RemoteSFTPLifecycleServiceStub: RemoteFileServicing {
    let suspendFirstListing: Bool
    let failListingAfterDelete: Bool
    private var listingCount = 0
    private var firstListingStarted = false
    private var firstListingContinuation: CheckedContinuation<Void, Never>?
    private var didDelete = false
    private(set) var deletedPaths: [String] = []

    init(
        suspendFirstListing: Bool = false,
        failListingAfterDelete: Bool = false
    ) {
        self.suspendFirstListing = suspendFirstListing
        self.failListingAfterDelete = failListingAfterDelete
    }

    func initialDirectory(connectionID: String) async throws -> String { "/srv" }

    func listDirectory(
        connectionID: String,
        path: String
    ) async throws -> RemoteDirectoryListing {
        let current = listingCount
        listingCount += 1
        if current == 0, suspendFirstListing {
            firstListingStarted = true
            await withCheckedContinuation { firstListingContinuation = $0 }
        }
        if didDelete, failListingAfterDelete {
            throw RemoteSFTPLifecycleTestError.refreshFailed
        }
        return .init(
            path: path,
            parentPath: path == "/srv" ? "/" : "/srv",
            entries: [
                .init(
                    name: "file.txt",
                    path: "\(path)/file.txt",
                    kind: .file,
                    size: 1,
                    modifiedAt: nil,
                    permissions: nil
                ),
            ]
        )
    }

    func uploadFile(
        connectionID: String,
        localURL: URL,
        remoteDirectory: String,
        overwrite: Bool
    ) async throws -> String { remoteDirectory }

    func downloadFile(
        connectionID: String,
        remotePath: String,
        localURL: URL,
        overwrite: Bool
    ) async throws {}

    func createDirectory(connectionID: String, parentPath: String, name: String) async throws {}
    func renameEntry(connectionID: String, path: String, newName: String) async throws {}
    func deleteEntry(connectionID: String, path: String, recursively: Bool) async throws {
        deletedPaths.append(path)
        didDelete = true
    }

    func waitForFirstListing() async {
        while !firstListingStarted { await Task.yield() }
    }

    func resumeFirstListing() {
        firstListingContinuation?.resume()
        firstListingContinuation = nil
    }
}
