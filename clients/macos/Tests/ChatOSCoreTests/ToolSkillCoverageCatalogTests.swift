import ChatOSCore
import XCTest

final class ToolSkillCoverageCatalogTests: XCTestCase {
    func testBundledSkillInventoryAndCoverageReferencesAreValid() throws {
        try BundledAgentSkillLoader.validateCatalog()

        let names = Set(BundledAgentSkillCatalog.skills.map(\.name))
        XCTAssertTrue(names.contains("chatos-terminal"))
        XCTAssertTrue(names.contains("chatos-project-files"))
        XCTAssertTrue(names.contains("chatos-compact-communication"))
        XCTAssertTrue(names.contains("requirement-survey"))

        for binding in ToolSkillCoverageCatalog.product.bindings {
            XCTAssertTrue(names.contains(binding.routerSkillName), binding.id)
            XCTAssertTrue(names.contains(binding.specialistSkillName), binding.id)
        }

        let execution = try BundledAgentSkillLoader.load(
            named: "chatos-terminal-command-execution"
        )
        XCTAssertEqual(execution.resourcePaths, ["references/scenarios.md"])
        XCTAssertTrue(execution.instructions.contains("foreground"))

        let observation = try BundledAgentSkillLoader.load(
            named: "chatos-terminal-process-observation"
        )
        XCTAssertTrue(observation.instructions.contains("process_poll"))
        let observationScenario = try BundledAgentSkillLoader.readResource(
            skillName: observation.descriptor.name,
            relativePath: "references/scenarios.md",
            maximumCharacters: 200
        )
        XCTAssertTrue(observationScenario.content.contains("Process observation scenarios"))
        XCTAssertTrue(observationScenario.truncated)

        let control = try BundledAgentSkillLoader.load(
            named: "chatos-terminal-process-control"
        )
        XCTAssertTrue(control.instructions.contains("process_kill"))

        let projectWrite = try BundledAgentSkillLoader.load(named: "chatos-project-write")
        XCTAssertEqual(
            projectWrite.resourcePaths,
            ["references/transactions-and-conflicts.md"]
        )
        XCTAssertTrue(projectWrite.instructions.contains("commit_edit_session"))
    }

    func testTerminalBindingsCoverEveryDeclaredToolExactlyOnce() {
        let bindings = ToolSkillCoverageCatalog.product.bindings.filter {
            $0.providerID == ProductToolProviderID.terminal
        }
        let tools = bindings.flatMap { binding in
            binding.toolNames.map { toolName in
                ToolSkillCoverageInput(
                    providerID: binding.providerID,
                    toolName: toolName,
                    skillBindingID: binding.id
                )
            }
        }
        let report = ToolSkillCoverageCatalog.product.audit(tools)

        XCTAssertEqual(tools.count, 9)
        XCTAssertEqual(Set(tools.map(\.toolName)).count, 9)
        XCTAssertEqual(report.coveredTools, 9)
        XCTAssertTrue(report.isComplete)
        XCTAssertTrue(report.issues.isEmpty)
    }

    func testProductCatalogCoversAllNativeBuiltinFamiliesWithoutToolOverlap() {
        let bindings = ToolSkillCoverageCatalog.product.bindings
        let providerTools = bindings.flatMap { binding in
            binding.toolNames.map { binding.providerID + ":" + $0 }
        }

        XCTAssertEqual(providerTools.count, 28)
        XCTAssertEqual(Set(providerTools).count, 28)
        XCTAssertEqual(
            bindings.first {
                $0.id == ProductToolSkillBindingID.requirementSurveyControlPlane
            }?.activationPolicy,
            .controlPlane
        )
    }

    func testAuditDiagnosesMissingUnknownAndMismatchedBindingsWithoutEnforcement() {
        let report = ToolSkillCoverageCatalog.product.audit([
            .init(providerID: nil, toolName: "a", skillBindingID: nil),
            .init(
                providerID: ProductToolProviderID.projectRead,
                toolName: "b",
                skillBindingID: nil
            ),
            .init(
                providerID: ProductToolProviderID.terminal,
                toolName: "c",
                skillBindingID: "missing"
            ),
            .init(
                providerID: ProductToolProviderID.projectRead,
                toolName: "execute_command",
                skillBindingID: ProductToolSkillBindingID.terminalCommandExecution
            ),
            .init(
                providerID: ProductToolProviderID.terminal,
                toolName: "not_execute_command",
                skillBindingID: ProductToolSkillBindingID.terminalCommandExecution
            ),
        ])

        XCTAssertEqual(report.coveredTools, 0)
        XCTAssertEqual(report.issues.map(\.kind), [
            .missingProviderID,
            .missingBindingID,
            .unknownBinding,
            .providerMismatch,
            .toolNotDeclared,
        ])
        XCTAssertFalse(report.isComplete)
    }
}
