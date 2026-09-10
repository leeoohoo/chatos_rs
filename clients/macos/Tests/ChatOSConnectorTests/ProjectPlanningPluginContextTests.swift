import Foundation
import Testing
@testable import ChatOSConnector

@Suite
struct ProjectManagementPluginContextTests {
    @Test("real project management manifest shares UI/MCP storage, isolates identity and rejects missing project")
    func strictProjectContext() throws {
        var repository = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { repository.deleteLastPathComponent() }
        let manifest = try JSONDecoder().decode(
            NativePluginManifest.self,
            from: Data(contentsOf: repository.appendingPathComponent("plugins/project-management/chatos.plugin.json"))
        )
        let root = URL(fileURLWithPath: "/tmp/chatos-project-plugin-context-test", isDirectory: true)
        func resolve(_ component: String, owner: String = "owner-a", project: String? = "project-a", name: String = "Project") throws -> NativeResolvedPluginRuntimeContext {
            try NativePluginRuntimeContextResolver.resolve(
                manifest: manifest,
                componentKey: component,
                runtimeRootURL: root,
                pluginID: "chatos-project-management",
                host: .init(
                    ownerUserID: owner,
                    deviceID: "device-a",
                    workspaceID: nil,
                    workspaceRoot: nil,
                    projectID: project,
                    projectName: name
                )
            )
        }
        let mcp = try resolve("project-management-mcp")
        let ui = try resolve("project-management-studio")
        #expect(mcp.dataURL == ui.dataURL)
        #expect(mcp.cacheURL == ui.cacheURL)
        #expect(mcp.environment == ui.environment)
        #expect(mcp.environment["CHATOS_PROJECT_ID"] == "project-a")
        #expect(mcp.environment["CHATOS_CONTEXT_SCOPE"] == "project")
        #expect(mcp.environment["CHATOS_WORKSPACE"] == nil)
        let renamed = try resolve("project-management-studio", name: "Renamed Project")
        #expect(renamed.dataURL == ui.dataURL)
        #expect(renamed.environment["CHATOS_PROJECT_NAME"] == "Renamed Project")
        let otherOwner = try resolve("project-management-studio", owner: "owner-b")
        let otherProject = try resolve("project-management-studio", project: "project-b")
        #expect(otherOwner.dataURL != ui.dataURL)
        #expect(otherProject.dataURL != ui.dataURL)
        let applicationID = "chatos-project-management:project-management-studio"
        #expect(otherOwner.websiteDataStoreID(applicationID: applicationID) != ui.websiteDataStoreID(applicationID: applicationID))
        #expect(otherProject.websiteDataStoreID(applicationID: applicationID) != ui.websiteDataStoreID(applicationID: applicationID))
        for component in ["project-management-mcp", "project-management-studio"] {
            #expect(throws: (any Error).self) { try resolve(component, project: nil) }
            #expect(throws: (any Error).self) { try resolve(component, project: "  ") }
        }
    }
}
