import Foundation
import Testing
@testable import ChatOSCore

@Suite("Remote file application boundary")
struct RemoteFileUseCaseTests {
    @Test
    func lifecycleKeepsLoadLanesIndependentAndMutationInvalidatesBoth() {
        var state = RemoteFileLifecycleState()
        let remote = state.beginRemoteLoad(connectionID: "c", target: .remote(path: "/old"))!
        let local = state.beginLocalLoad(connectionID: "c", path: "/tmp")!
        let newerRemote = state.beginRemoteLoad(connectionID: "c", target: .remote(path: "/new"))!

        #expect(!state.acceptsRemoteLoad(remote))
        #expect(state.acceptsRemoteLoad(newerRemote))
        #expect(state.acceptsLocalLoad(local))

        let mutation = state.beginMutation(
            connectionID: "c",
            kind: .delete(path: "/new/file")
        )!
        #expect(!state.acceptsRemoteLoad(newerRemote))
        #expect(!state.acceptsLocalLoad(local))
        #expect(state.acceptsMutation(mutation))
        #expect(state.beginRemoteLoad(connectionID: "c", target: .remote(path: "/blocked")) == nil)
    }

    @Test
    func committedMutationReturnsMissingRefreshInsteadOfThrowing() async throws {
        let service = RemoteFileUseCaseStub(failListingAfterMutation: true)
        let useCase = RemoteFileUseCase(service: service)

        let result = try await useCase.delete(
            connectionID: "c",
            path: "/srv/old",
            recursively: false,
            parentPath: "/srv"
        )

        #expect(result.refreshedRemoteListing == nil)
        #expect(await service.deletedPaths == ["/srv/old"])
    }

    @Test
    func connectAuthenticatesBeforeResolvingAndListingInitialDirectory() async throws {
        let service = RemoteFileUseCaseStub()
        let useCase = RemoteFileUseCase(service: service)

        let listing = try await useCase.connect(
            connectionID: "c",
            verificationCode: "654321"
        )

        #expect(listing.path == "/srv")
        #expect(await service.calls == ["authenticate:654321", "initial", "list:/srv"])
    }
}

private enum RemoteFileUseCaseTestError: Error {
    case refreshFailed
}

private actor RemoteFileUseCaseStub: RemoteFileServicing {
    let failListingAfterMutation: Bool
    private(set) var calls: [String] = []
    private(set) var deletedPaths: [String] = []
    private var didMutate = false

    init(failListingAfterMutation: Bool = false) {
        self.failListingAfterMutation = failListingAfterMutation
    }

    func authenticate(connectionID: String, verificationCode: String?) async throws {
        calls.append("authenticate:\(verificationCode ?? "nil")")
    }

    func initialDirectory(connectionID: String) async throws -> String {
        calls.append("initial")
        return "/srv"
    }

    func listDirectory(
        connectionID: String,
        path: String
    ) async throws -> RemoteDirectoryListing {
        calls.append("list:\(path)")
        if didMutate, failListingAfterMutation { throw RemoteFileUseCaseTestError.refreshFailed }
        return .init(path: path, parentPath: nil, entries: [])
    }

    func uploadFile(
        connectionID: String,
        localURL: URL,
        remoteDirectory: String,
        overwrite: Bool
    ) async throws -> String {
        didMutate = true
        return remoteDirectory
    }

    func downloadFile(
        connectionID: String,
        remotePath: String,
        localURL: URL,
        overwrite: Bool
    ) async throws {
        didMutate = true
    }

    func createDirectory(connectionID: String, parentPath: String, name: String) async throws {
        didMutate = true
    }

    func renameEntry(connectionID: String, path: String, newName: String) async throws {
        didMutate = true
    }

    func deleteEntry(connectionID: String, path: String, recursively: Bool) async throws {
        deletedPaths.append(path)
        didMutate = true
    }
}
