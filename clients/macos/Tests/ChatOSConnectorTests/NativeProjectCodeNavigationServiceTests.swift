@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeProjectCodeNavigationServiceTests: XCTestCase {
    func testFindsDefinitionAndReferencesInsideNativeWorkspace() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("chatos-code-nav-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data("export const categoryApi = { list() {} }\n".utf8)
            .write(to: root.appendingPathComponent("categories.ts"))
        try Data("categoryApi.list()\n".utf8)
            .write(to: root.appendingPathComponent("consumer.ts"))

        let stateURL = root.appendingPathComponent("connector-state.json")
        var state = NativeConnectorPersistentState.empty
        state.deviceID = "device"
        state.workspaces = [
            .init(id: "workspace", alias: "test", absoluteRoot: root.path, fingerprint: "test"),
        ]
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let connector = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: stateURL
            ),
            ticketProvider: NavigationTicketProvider()
        )
        let service = NativeProjectCodeNavigationService(connector: connector)
        let projectRoot = "local://connector/device/workspace"

        let definition = try await service.definition(
            projectRoot: projectRoot,
            filePath: projectRoot + "/consumer.ts",
            line: 1,
            column: 2
        )
        XCTAssertEqual(definition.token, "categoryApi")
        XCTAssertEqual(definition.locations.first?.relativePath, "categories.ts")
        XCTAssertEqual(definition.locations.first?.line, 1)

        let references = try await service.references(
            projectRoot: projectRoot,
            filePath: projectRoot + "/categories.ts",
            line: 1,
            column: 15
        )
        XCTAssertEqual(references.token, "categoryApi")
        XCTAssertEqual(references.locations.map(\.relativePath), ["consumer.ts"])
    }
}

private struct NavigationTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused" }
}
