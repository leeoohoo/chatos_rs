import Darwin
import Foundation
import Testing
@testable import ChatOSApp

@Suite("Local Agent runtime configuration")
struct RuntimeConfigurationTests {
    @Test("account IPC socket remains within the macOS Unix socket path limit")
    func localAgentSocketPathIsBounded() {
        let settings = RuntimeConfiguration.localAgentBootstrapSettings(
            accountID: "account-\(String(repeating: "a", count: 500))",
            deviceID: "device-0123456789abcdef0123456789abcdef"
        )
        let longestGeneratedSocketName = "agent-\(String(repeating: "f", count: 36)).sock"
        let socketPath = settings.runtimeDirectory
            .appendingPathComponent(longestGeneratedSocketName)
            .path

        #expect(socketPath.utf8CString.count <= MemoryLayout.size(ofValue: sockaddr_un().sun_path))
        #expect(settings.runtimeDirectory.path.hasPrefix("/tmp/chatos-la-\(geteuid())/"))
        #expect(!settings.platformStateDirectory.path.hasPrefix("/tmp/"))
    }
}
