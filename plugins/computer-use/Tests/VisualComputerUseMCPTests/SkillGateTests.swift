import MCP
import Testing
@testable import VisualComputerUseMCP

@Test func everyComputerUseToolDeclaresRouterAndSpecialistSkillGate() throws {
    #expect(!MCPService.tools.isEmpty)

    for tool in MCPService.tools {
        let schema = try #require(tool.inputSchema.objectValue)
        let properties = try #require(schema["properties"]?.objectValue)
        #expect(properties["skillEvidence"] == nil)

        let required = schema["required"]?.arrayValue ?? []
        #expect(!required.contains(.string("skillEvidence")))

        let gate = try #require(tool._meta?["chatos/skillGate"]?.objectValue)
        let skills = try #require(gate["allOf"]?.arrayValue)
        #expect(skills.count == 2)
        #expect(skills.contains(.string("visual-computer-use")))
    }
}

@Test func zeroDisplayIDUsesAutomaticDisplaySelection() {
    #expect(MCPService.normalizedDisplayID(nil) == nil)
    #expect(MCPService.normalizedDisplayID(0) == nil)
    #expect(MCPService.normalizedDisplayID(42) == 42)
}
