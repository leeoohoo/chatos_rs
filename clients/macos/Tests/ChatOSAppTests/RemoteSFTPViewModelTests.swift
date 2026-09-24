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

        await viewModel.load()

        #expect(viewModel.verificationPrompt == "Please Input Mfa Code (SMS):")
        #expect(viewModel.remoteEntries.isEmpty)

        viewModel.verificationCode = "614207"
        await viewModel.submitVerification()

        #expect(viewModel.verificationPrompt == nil)
        #expect(viewModel.errorMessage == nil)
        #expect(viewModel.remotePath == "/srv/app")
        #expect(viewModel.remoteEntries.map(\.name) == ["app.log"])
        #expect(await service.submittedCodes() == [nil, "614207"])
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
