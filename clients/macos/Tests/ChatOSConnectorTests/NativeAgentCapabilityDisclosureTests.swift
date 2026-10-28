@testable import ChatOSConnector
import ChatOSAgentRuntime
import XCTest

final class NativeAgentCapabilityDisclosureTests: XCTestCase {
    func testAdvertisedOptionalArgumentsAcceptOmissionAndNullWithoutPlaceholders() throws {
        XCTAssertNil(
            NativeAgentCapabilityBrokerToolCatalog.definitions.first {
                $0.name == "agent_skill_list_resources"
            },
            "Activation already returns declared resources; progressive disclosure must not advertise a redundant list step"
        )
        let definition = try XCTUnwrap(NativeAgentCapabilityBrokerToolCatalog.definitions.first {
            $0.name == "capability_describe"
        })
        for arguments in [
            #"{"plugin_option":"plugin_1"}"#,
            #"{"plugin_option":"plugin_1","tool_option":null,"query":"session_open navigate snapshot","offset":0,"limit":8,"schema_offset":null}"#,
            #"{"plugin_option":"plugin_1","tool_option":"tool_27","query":null,"offset":null,"limit":null,"schema_offset":null}"#,
        ] {
            try AgentSchemaValidator.validate(arguments: arguments, schema: definition.schema)
            try AgentExternalToolSchemaValidator.validate(arguments: arguments, schema: definition.schema)
        }
        for arguments in [
            #"{"plugin_option":"plugin_1","tool_option":42}"#,
            #"{"plugin_option":"plugin_1","limit":0}"#,
            #"{"plugin_option":"plugin_1","schema_offset":-1}"#,
        ] {
            XCTAssertThrowsError(try AgentSchemaValidator.validate(arguments: arguments, schema: definition.schema))
            XCTAssertThrowsError(try AgentExternalToolSchemaValidator.validate(arguments: arguments, schema: definition.schema))
        }
        guard case let .object(tool)? = NativeAgentCapabilityBrokerToolCatalog.localAgentCapabilityTools.first(where: {
            guard case let .object(object) = $0 else { return false }
            return object["name"] == .string("capability_describe")
        }) else { return XCTFail("Missing describe tool") }
        XCTAssertEqual(tool["strict"], .bool(false))

        let activate = try XCTUnwrap(NativeAgentCapabilityBrokerToolCatalog.definitions.first {
            $0.name == "agent_skill_activate"
        })
        try AgentSchemaValidator.validate(
            arguments: #"{"skill_ref":"plugin-skill:plugin_1:router"}"#,
            schema: activate.schema
        )
        XCTAssertThrowsError(try AgentSchemaValidator.validate(
            arguments: #"{"plugin_option":"plugin_1","skill_name":"router"}"#,
            schema: activate.schema
        ))

        let read = try XCTUnwrap(NativeAgentCapabilityBrokerToolCatalog.definitions.first {
            $0.name == "agent_skill_read_resource"
        })
        let readSchema = try JSONDecoder().decode(
            NativeJSONValue.self,
            from: read.schema
        ).jsonObject
        XCTAssertEqual(
            readSchema?["properties"]?.jsonObject?["max_chars"]?.jsonObject?["maximum"],
            .number(160_000)
        )
        XCTAssertEqual(
            readSchema?["properties"]?.jsonObject?["max_chars"]?.jsonObject?["default"],
            .number(160_000)
        )
        XCTAssertThrowsError(try AgentSchemaValidator.validate(
            arguments: #"{"skill_ref":"plugin-skill:plugin_1:router","relative_path":"references/x.md","limit":1000}"#,
            schema: read.schema
        ))
    }

    private func descriptor(count: Int = 39, schema: NativeJSONValue? = nil) -> DescribeResponse {
        .init(bindingSchemaVersion: 1, providerID: "plugin:test", pluginOption: "plugin_1", name: "Test",
              tools: (1...count).map { index in
                  .init(toolOption: "tool_\(index)", name: index == 27 ? "browser_session_open" : "operation_\(index)",
                        description: "A real operation \(index)",
                        inputSchema: schema ?? .object(["type": .string("object"), "properties": .object([
                            "value": .object(["type": .string("string")]),
                        ])]), effect: "write", requiredSkillRefs: ["plugin-skill:plugin_1:router"],
                        invocationMode: "capability_invoke",
                        skillGate: .object(["all_of": .array([.string("plugin-skill:plugin_1:router")])]))
              }, skills: [.init(skillRef: "plugin-skill:plugin_1:router", name: "router", role: "router", description: "Route")])
    }

    private func checked(_ result: NativeJSONValue) throws -> [String: NativeJSONValue] {
        XCTAssertLessThanOrEqual(result.canonicalJSONString.unicodeScalars.count,
                                 NativeAgentCapabilityDisclosure.maximumResponseCharacters)
        XCTAssertEqual(try JSONDecoder().decode(NativeJSONValue.self, from: Data(result.canonicalJSONString.utf8)), result)
        XCTAssertFalse(result.canonicalJSONString.contains("...[truncated"))
        return try XCTUnwrap(result.jsonObject)
    }

    func testLargeCatalogBatchesFrozenSchemasWithinBudgetAndKeepsStableTokens() throws {
        let source = descriptor()
        var offset = 0
        var tokens: [String] = []
        var completeSchemaCount = 0
        repeat {
            let page = try checked(NativeAgentCapabilityDisclosure.response(source, offset: offset))
            XCTAssertEqual(page["view"], .string("catalog"))
            let tools = try XCTUnwrap(page["tools"]?.jsonArray)
            XCTAssertLessThanOrEqual(tools.count, 8)
            for tool in tools {
                if tool.jsonObject?["input_schema"] != nil {
                    completeSchemaCount += 1
                    XCTAssertNotNil(tool.jsonObject?["schema_sha256"])
                    XCTAssertNotNil(tool.jsonObject?["schema_total_chars"])
                    XCTAssertNotNil(tool.jsonObject?["skill_gate"])
                }
                tokens.append(try XCTUnwrap(tool.jsonObject?["tool_option"]?.jsonString))
            }
            if case let .number(next)? = page["next_offset"] { offset = Int(next) } else { break }
        } while true
        XCTAssertEqual(tokens, (1...39).map { "tool_\($0)" })
        XCTAssertGreaterThan(completeSchemaCount, 0)
        let search = try checked(NativeAgentCapabilityDisclosure.response(source, query: "browser_session_open"))
        XCTAssertEqual(search["tools"]?.jsonArray?.count, 1)
        XCTAssertEqual(search["tools"]?.jsonArray?.first?.jsonObject?["tool_option"], .string("tool_27"))
        let detail = try checked(NativeAgentCapabilityDisclosure.response(source, toolOption: "tool_27"))
        XCTAssertEqual(detail["view"], .string("tool"))
        XCTAssertEqual(detail["tools"]?.jsonArray?.count, 1)
        XCTAssertEqual(detail["tools"]?.jsonArray?.first?.jsonObject?["input_schema"], source.tools[26].inputSchema)
        XCTAssertNotNil(detail["tools"]?.jsonArray?.first?.jsonObject?["skill_gate"])
    }

    func testCatalogDisclosesOnlySkillsRequiredByReturnedToolPage() throws {
        let source = DescribeResponse(
            bindingSchemaVersion: 1,
            providerID: "plugin:test",
            pluginOption: "plugin_1",
            name: "Test",
            tools: [
                .init(
                    toolOption: "tool_1", name: "read_document", description: "Read one document",
                    inputSchema: .object(["type": .string("object")]), effect: "read",
                    requiredSkillRefs: ["plugin-skill:plugin_1:read"],
                    invocationMode: "capability_invoke",
                    skillGate: .object(["all_of": .array([.string("plugin-skill:plugin_1:read")])])
                ),
                .init(
                    toolOption: "tool_2", name: "write_document", description: "Write one document",
                    inputSchema: .object(["type": .string("object")]), effect: "write",
                    requiredSkillRefs: ["plugin-skill:plugin_1:write"],
                    invocationMode: "capability_invoke",
                    skillGate: .object(["all_of": .array([.string("plugin-skill:plugin_1:write")])])
                ),
            ],
            skills: [
                .init(skillRef: "plugin-skill:plugin_1:read", name: "read", role: "leaf", description: "Read"),
                .init(skillRef: "plugin-skill:plugin_1:write", name: "write", role: "leaf", description: "Write"),
            ]
        )

        let result = try checked(
            NativeAgentCapabilityDisclosure.response(source, query: "read_document")
        )
        XCTAssertEqual(
            result["skills"]?.jsonArray?.compactMap { $0.jsonObject?["skill_ref"]?.jsonString },
            ["plugin-skill:plugin_1:read"]
        )
    }

    func testHugeUnicodeSchemaUsesExplicitLosslessPages() throws {
        let schema: NativeJSONValue = .object(["type": .string("object"), "description": .string(
            String(repeating: "A😀e\u{301}中\"\\\n", count: 4_000)
        )])
        let source = descriptor(count: 1, schema: schema)
        let initial = try checked(NativeAgentCapabilityDisclosure.response(source, toolOption: "tool_1"))
        let tool = try XCTUnwrap(initial["tools"]?.jsonArray?.first?.jsonObject)
        XCTAssertNil(tool["input_schema"])
        XCTAssertEqual(tool["schema_delivery"], .string("paged"))
        var offset = 0
        var reconstructed = ""
        repeat {
            let page = try checked(NativeAgentCapabilityDisclosure.response(source, toolOption: "tool_1", schemaOffset: offset))
            let entry = try XCTUnwrap(page["tools"]?.jsonArray?.first?.jsonObject)
            XCTAssertEqual(entry["schema_sha256"], tool["schema_sha256"])
            XCTAssertEqual(entry["schema_offset"], .number(Double(offset)))
            let chunk = try XCTUnwrap(entry["schema_chunk"]?.jsonString)
            reconstructed += chunk
            if case let .number(next)? = entry["next_schema_offset"] {
                XCTAssertEqual(Int(next), offset + chunk.unicodeScalars.count)
                offset = Int(next)
            } else { break }
        } while true
        XCTAssertEqual(reconstructed, schema.canonicalJSONString)
        XCTAssertEqual(try JSONDecoder().decode(NativeJSONValue.self, from: Data(reconstructed.utf8)), schema)
    }

    func testUnknownTokensAndInvalidPaginationFailWithoutGuessing() throws {
        let source = descriptor()
        for operation in [
            { try NativeAgentCapabilityDisclosure.response(source, toolOption: "tool_400") },
            { try NativeAgentCapabilityDisclosure.response(source, offset: -1) },
            { try NativeAgentCapabilityDisclosure.response(source, limit: 21) },
            { try NativeAgentCapabilityDisclosure.response(source, schemaOffset: 0) },
            { try NativeAgentCapabilityDisclosure.response(source, toolOption: "tool_1", query: "other") },
            { try NativeAgentCapabilityDisclosure.response(source, toolOption: "tool_1", schemaOffset: 100_000) },
        ] {
            XCTAssertThrowsError(try operation())
        }
        let empty = try checked(NativeAgentCapabilityDisclosure.response(source, query: "not_available"))
        XCTAssertEqual(empty["tools"], .array([]))
        XCTAssertEqual(empty["next_offset"], .null)
    }

    func testOversizedMetadataGetsConcreteErrorNotBrokenJSON() throws {
        let source = DescribeResponse(bindingSchemaVersion: 1, providerID: "plugin:test", pluginOption: "plugin_1",
            name: "Test", tools: descriptor(count: 1).tools, skills: [
                .init(skillRef: "plugin-skill:plugin_1:router", name: "router", role: "router",
                      description: String(repeating: "很长", count: 10_000)),
            ])
        let result = try checked(NativeAgentCapabilityDisclosure.response(source))
        XCTAssertEqual(result["view"], .string("error"))
        XCTAssertEqual(result["error"], .string("capability_description_entry_too_large"))
    }
}

/// Integration fixtures inspect all definitions explicitly; production agents select only needed tools.
func readCapabilityTestDefinitions(_ provider: any AgentToolProvider, pluginOption: String) async throws -> [NativeJSONValue] {
    var offset = 0
    var indices: [NativeJSONValue] = []
    repeat {
        let response = try await provider.execute(.init(id: "index-\(offset)", name: "capability_describe",
            arguments: NativeJSONValue.object(["plugin_option": .string(pluginOption), "offset": .number(Double(offset))]).canonicalJSONString))
        let page = try JSONDecoder().decode(NativeJSONValue.self, from: Data(response.content.utf8)).jsonObject
        indices += page?["tools"]?.jsonArray ?? []
        if case let .number(next)? = page?["next_offset"] { offset = Int(next) } else { break }
    } while true
    var tools: [NativeJSONValue] = []
    for entry in indices {
        if entry.jsonObject?["input_schema"] != nil,
           entry.jsonObject?["schema_sha256"] != nil,
           entry.jsonObject?["skill_gate"] != nil {
            tools.append(entry)
            continue
        }
        let token = try XCTUnwrap(entry.jsonObject?["tool_option"]?.jsonString)
        let response = try await provider.execute(.init(id: "detail-\(token)", name: "capability_describe",
            arguments: NativeJSONValue.object(["plugin_option": .string(pluginOption), "tool_option": .string(token)]).canonicalJSONString))
        let detail = try JSONDecoder().decode(NativeJSONValue.self, from: Data(response.content.utf8)).jsonObject
        tools += detail?["tools"]?.jsonArray ?? []
    }
    return tools
}
