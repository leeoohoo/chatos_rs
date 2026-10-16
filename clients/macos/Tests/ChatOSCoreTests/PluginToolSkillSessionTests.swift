import ChatOSCore
import XCTest

final class PluginToolSkillSessionTests: XCTestCase {
    func testProgressiveSkillPagesUseUnifiedOneHundredSixtyThousandCharacterBudget() throws {
        let text = String(repeating: "界", count: 160_001)
        let page = try ProgressiveSkillFileLoader.textPage(
            text,
            offset: 0,
            maximumCharacters: .max
        )

        XCTAssertEqual(page.content.count, 160_000)
        XCTAssertEqual(page.nextOffset, 160_000)
        XCTAssertTrue(page.truncated)
    }

    func testSharedPluginSkillGateFixtureMatchesSwiftRuntime() throws {
        var repositoryRoot = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { repositoryRoot.deleteLastPathComponent() }
        let fixtureURL = repositoryRoot.appendingPathComponent(
            "crates/chatos_plugin_management_sdk/fixtures/plugin_skill_gate_v1.json"
        )
        let fixture = try XCTUnwrap(
            try JSONSerialization.jsonObject(with: Data(contentsOf: fixtureURL))
                as? [String: Any]
        )
        XCTAssertEqual(fixture["schema_version"] as? Int, 1)
        let cases = try XCTUnwrap(fixture["cases"] as? [[String: Any]])

        for fixtureCase in cases {
            let identifier = try XCTUnwrap(fixtureCase["id"] as? String)
            do {
                let gate = try PluginToolSkillGate.decode(
                    JSONSerialization.data(withJSONObject: try XCTUnwrap(fixtureCase["gate"]))
                )
                let required = try gate.requiredSkillNames(
                    arguments: JSONSerialization.data(
                        withJSONObject: try XCTUnwrap(fixtureCase["arguments"]),
                        options: [.fragmentsAllowed]
                    )
                )
                if let expectedError = fixtureCase["expected_error"] as? String {
                    XCTFail("Fixture \(identifier) expected \(expectedError)")
                } else {
                    XCTAssertEqual(
                        gate.catalogSkillNames,
                        fixtureCase["expected_catalog_skills"] as? [String],
                        identifier
                    )
                    XCTAssertEqual(
                        required,
                        fixtureCase["expected_required_skills"] as? [String],
                        identifier
                    )
                }
            } catch {
                let code = (error as? PluginToolSkillGate.GateError)?.code
                    ?? PluginToolSkillGate.GateError.invalidDeclaration.code
                XCTAssertEqual(code, fixtureCase["expected_error"] as? String, identifier)
            }
        }
    }

    func testSnapshotAndDynamicGateStayRunScopedAndProgressive() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("plugin-skill-session-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }

        try writeSkill(
            root: root,
            name: "demo-router",
            role: "router",
            body: "# Demo router\n\nChoose one leaf."
        )
        try writeSkill(
            root: root,
            name: "demo-read",
            role: "leaf",
            body: "# Demo read\n\nRead narrowly.",
            reference: "read example"
        )
        let agentMetadata = root.appendingPathComponent(
            "skills/demo-read/agents",
            isDirectory: true
        )
        try FileManager.default.createDirectory(
            at: agentMetadata,
            withIntermediateDirectories: true
        )
        try Data("interface:\n  display_name: Demo".utf8).write(
            to: agentMetadata.appendingPathComponent("openai.yaml")
        )
        try Data(#"{"internal":true}"#.utf8).write(
            to: root.appendingPathComponent("skills/demo-read/contract.json")
        )
        let scripts = root.appendingPathComponent(
            "skills/demo-read/scripts",
            isDirectory: true
        )
        try FileManager.default.createDirectory(at: scripts, withIntermediateDirectories: true)
        try Data("internal helper".utf8).write(
            to: scripts.appendingPathComponent("helper.txt")
        )
        try writeSkill(
            root: root,
            name: "demo-write",
            role: "leaf",
            body: "# Demo write\n\nWrite carefully."
        )

        let snapshot = try PluginSkillSnapshot.load(
            installationRoot: root,
            relativeSkillDirectories: [
                "skills/demo-router", "skills/demo-read", "skills/demo-write",
            ]
        )
        let gate = try PluginToolSkillGate.decode(Data(#"""
        {
          "allOf":["demo-router"],
          "selectByArgument":{
            "pointer":"/mode",
            "map":{"read":"demo-read","write":"demo-write"}
          }
        }
        """#.utf8))
        XCTAssertEqual(
            gate.catalogSkillNames,
            ["demo-read", "demo-router", "demo-write"]
        )

        let session = PluginToolSkillSession(snapshot: snapshot)
        let readDocument = try snapshot.document(named: "demo-read")
        XCTAssertEqual(
            readDocument.resourcePaths,
            ["references/example.md"],
            "Only references/ is model-readable; UI metadata, executable helpers, assets and internal contracts stay package-private"
        )
        let readArguments = Data(#"{"mode":"read"}"#.utf8)
        let missingBefore = try await session.missingSkills(
            for: gate,
            arguments: readArguments
        )
        XCTAssertEqual(missingBefore, ["demo-read", "demo-router"])
        _ = try await session.activate(named: "demo-router")
        _ = try await session.activate(named: "demo-read")
        let missingAfter = try await session.missingSkills(
            for: gate,
            arguments: readArguments
        )
        XCTAssertEqual(missingAfter, [])

        let page = try await session.readResource(
            skillName: "demo-read",
            relativePath: "references/example.md",
            maximumCharacters: 4
        )
        XCTAssertEqual(page.content, "read")
        XCTAssertTrue(page.truncated)

        let skillURL = root.appendingPathComponent("skills/demo-read/SKILL.md")
        try Data("tampered".utf8).write(to: skillURL, options: .atomic)
        let frozen = try await session.activate(named: "demo-read")
        XCTAssertTrue(frozen.instructions.contains("Read narrowly."))
        XCTAssertFalse(frozen.instructions.contains("tampered"))
    }

    func testDynamicGateFailsClosedForMissingOrUnknownSelector() throws {
        let gate = try PluginToolSkillGate.decode(Data(#"""
        {
          "selectByArgument":{"pointer":"/kind","map":{"a":"demo-read"}}
        }
        """#.utf8))

        XCTAssertThrowsError(try gate.requiredSkillNames(arguments: Data(#"{}"#.utf8)))
        XCTAssertThrowsError(
            try gate.requiredSkillNames(arguments: Data(#"{"kind":"b"}"#.utf8))
        )
        XCTAssertThrowsError(
            try PluginToolSkillGate.decode(Data(#"{"allOf":["Bad Skill"]}"#.utf8))
        )
    }

    private func writeSkill(
        root: URL,
        name: String,
        role: String,
        body: String,
        reference: String? = nil
    ) throws {
        let directory = root.appendingPathComponent("skills/\(name)", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let markdown = """
        ---
        name: \(name)
        description: Test \(name)
        metadata:
          chatos.role: \(role)
        ---

        \(body)
        """
        try Data(markdown.utf8).write(to: directory.appendingPathComponent("SKILL.md"))
        if let reference {
            let references = directory.appendingPathComponent("references", isDirectory: true)
            try FileManager.default.createDirectory(
                at: references,
                withIntermediateDirectories: true
            )
            try Data(reference.utf8).write(to: references.appendingPathComponent("example.md"))
        }
    }
}
