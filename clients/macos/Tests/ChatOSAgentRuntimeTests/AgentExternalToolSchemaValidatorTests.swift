import ChatOSAgentRuntime
import Foundation
import XCTest

final class AgentExternalToolSchemaValidatorTests: XCTestCase {
    private func validate(_ arguments: String, _ schema: String) throws {
        try AgentExternalToolSchemaValidator.validate(arguments: arguments, schema: Data(schema.utf8))
    }

    func testRequiredFieldAndNestedTypeReportCorrectablePaths() throws {
        let schema = #"{"type":"object","properties":{"url":{"type":"string","minLength":1},"options":{"type":"object","properties":{"timeout":{"type":"integer","minimum":1}}}},"required":["url"],"additionalProperties":false}"#
        for (arguments, path) in [("{}", "$.url"), (#"{"url":12}"#, "$.url"),
            (#"{"url":""}"#, "$.url"), (#"{"url":"https://example.test","options":{"timeout":true}}"#, "$.options.timeout"),
            (#"{"url":"ok","unknown":1}"#, "$.unknown")] {
            XCTAssertThrowsError(try validate(arguments, schema)) { error in
                XCTAssertTrue(error.localizedDescription.contains(path), error.localizedDescription)
            }
        }
        XCTAssertNoThrow(try validate(#"{"url":"ok","options":{"timeout":100}}"#, schema))
    }

    func testUnionReferencesAndCombinatorsAcceptValidPluginSchemas() throws {
        let schema = ##"{"$schema":"https://json-schema.org/draft/2020-12/schema","$defs":{"query":{"type":["string","null"]}},"type":"object","properties":{"query":{"$ref":"#/$defs/query"},"action":{"oneOf":[{"type":"object","properties":{"kind":{"const":"read"}},"required":["kind"]},{"type":"object","properties":{"kind":{"const":"write"}},"required":["kind"]}]}},"required":["query","action"]}"##
        for arguments in [#"{"query":null,"action":{"kind":"read"}}"#, #"{"query":"find","action":{"kind":"write"}}"#] {
            XCTAssertNoThrow(try validate(arguments, schema))
        }
        XCTAssertThrowsError(try validate(#"{"query":false,"action":{"kind":"read"}}"#, schema))
        XCTAssertThrowsError(try validate(#"{"query":"test","action":{"kind":"remove"}}"#, schema))
        XCTAssertNoThrow(try validate("4", #"{"allOf":[{"type":"integer"},{"minimum":1}],"anyOf":[{"maximum":3},{"const":4}]}"#))
        XCTAssertThrowsError(try validate("5", #"{"allOf":[{"type":"integer"},{"minimum":1}],"anyOf":[{"maximum":3},{"const":4}]}"#))
    }

    func testArraysNullBooleanAndConstraints() throws {
        XCTAssertNoThrow(try validate(#"["a",1]"#, #"{"type":"array","prefixItems":[{"type":"string"}],"items":{"type":"integer"},"minItems":2}"#))
        XCTAssertThrowsError(try validate(#"["a","b"]"#, #"{"type":"array","prefixItems":[{"type":"string"}],"items":{"type":"integer"}}"#))
        XCTAssertThrowsError(try validate("[true,1]", #"{"type":"array","items":{"type":"integer"}}"#))
        XCTAssertThrowsError(try validate("true", #"{"enum":[1]}"#))
        XCTAssertNoThrow(try validate("null", #"{"type":["string","null"]}"#))
        XCTAssertNoThrow(try validate(#""abc""#, #"{"type":"string","pattern":"^a","maxLength":3}"#))
        XCTAssertThrowsError(try validate(#""xbc""#, #"{"type":"string","pattern":"^a"}"#))
        XCTAssertThrowsError(try validate("[1,1]", #"{"type":"array","uniqueItems":true}"#))
    }

    func testUnsupportedSchemaAndRemoteReferencesNeverPassSilently() throws {
        for schema in [#"{"$ref":"https://example.test/schema"}"#, #"{"unknownAssertion":true}"#,
            ##"{"$ref":"#"}"##, ##"{"$ref":"#/$defs/missing"}"##] {
            XCTAssertThrowsError(try validate("{}", schema)) { error in
                XCTAssertTrue(error.localizedDescription.contains("Schema 暂不支持"))
            }
        }
        XCTAssertThrowsError(try validate("not JSON", #"{"type":"object"}"#))
        XCTAssertThrowsError(try validate("{}", "false"))
        XCTAssertNoThrow(try validate("{}", "true"))
        XCTAssertNoThrow(try validate("{}", "{}"))
    }

    func testRegistryRejectsInvalidInnerArgumentsWithoutInvokingProvider() async throws {
        let provider = SchemaCountingProvider()
        let registry = try await AgentToolProviderRegistry(providers: [provider])
        for arguments in ["{}", #"{"query":false}"#, #"{"query":"ok","extra":1}"#] {
            let outcome = try await registry.execute(.init(id: "call", name: "document_search", arguments: arguments))
            XCTAssertTrue(outcome.isError)
            XCTAssertTrue(outcome.content.contains("document_search"))
            XCTAssertTrue(outcome.content.contains("$."))
        }
        let rejectedCount = await provider.count
        XCTAssertEqual(rejectedCount, 0)
        let success = try await registry.execute(.init(id: "call", name: "document_search", arguments: #"{"query":"ok"}"#))
        XCTAssertFalse(success.isError)
        let acceptedCount = await provider.count
        XCTAssertEqual(acceptedCount, 1)
    }
}

private actor SchemaCountingProvider: AgentToolProvider {
    var count = 0
    func definitions() async throws -> [AgentToolDefinition] {
        [.init(name: "document_search", description: "Search documents",
            schema: Data(#"{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}"#.utf8), effect: .readOnly)]
    }
    func execute(_ call: AgentToolCall) async throws -> AgentToolOutcome {
        count += 1
        return .init("verified")
    }
}
