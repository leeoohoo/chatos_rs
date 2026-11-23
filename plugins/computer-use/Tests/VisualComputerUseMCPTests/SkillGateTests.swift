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

@Test func macOSApplicationActivationAcceptsExactlyOneIdentifier() throws {
    let tool = try #require(MCPService.tools.first { $0.name == "activate_application" })
    let schema = try #require(tool.inputSchema.objectValue)
    let properties = try #require(schema["properties"]?.objectValue)
    #expect(properties["application"] != nil)
    #expect(properties["bundle_identifier"] != nil)

    let alternatives = try #require(schema["oneOf"]?.arrayValue)
    #expect(alternatives.count == 2)
    let requiredFields = alternatives.compactMap { alternative in
        alternative.objectValue?["required"]?.arrayValue?.first?.stringValue
    }
    #expect(Set(requiredFields) == ["application", "bundle_identifier"])
}
