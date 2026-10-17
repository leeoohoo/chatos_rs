@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentPlatformToolCatalogTests: XCTestCase {
    func testMainChatCanReadImagesWithoutAddingATaskBusinessTool() throws {
        let tools = NativeLocalAgentPlatformToolCatalog.capabilityTools
        let tool = try XCTUnwrap(tools.first { value in
            guard case let .object(object) = value else { return false }
            return object["name"] == .string("local_attachment_read")
        })
        guard case let .object(object) = tool,
              case let .object(parameters)? = object["parameters"],
              case let .object(properties)? = parameters["properties"],
              case let .object(delivery)? = properties["delivery"] else { return XCTFail("missing image schema") }
        XCTAssertEqual(delivery["default"], .string("model"))
        XCTAssertEqual(delivery["enum"], .array([.string("model"), .string("content")]))
        XCTAssertEqual(NativeLocalAgentPlatformToolCatalog.mainChatTaskToolNames.count, 8)
        XCTAssertFalse(NativeLocalAgentPlatformToolCatalog.mainChatTaskToolNames.contains("local_attachment_read"))
        let snapshot = try NativeLocalConnectorService.mainChatTaskSkillSnapshot()
        XCTAssertTrue(snapshot.instructions.contains("screenshot interpretation"))
        XCTAssertTrue(snapshot.instructions.contains("delivery=model"))
    }
    func testAcceptanceSchemaUpgradePreservesExistingCapabilityChoicesAndIsIdempotent() throws {
        for name in ["create_task", "create_tasks_with_prerequisites"] {
            let tool = try XCTUnwrap(NativeLocalAgentPlatformToolCatalog.capabilityTools.first { value in
                guard case .object(let definition) = value else { return false }
                return definition["name"] == .string(name)
            })
            let upgraded = NativeLocalAgentPlatformToolCatalog.upgradingTaskAcceptanceSchema(tool)
            XCTAssertEqual(upgraded, tool)
            XCTAssertEqual(NativeLocalAgentPlatformToolCatalog.upgradingTaskAcceptanceSchema(upgraded), upgraded)
            guard case .object(let definition) = tool,
                  case .object(let parameters)? = definition["parameters"],
                  case .object(let properties)? = parameters["properties"] else { return XCTFail("missing schema") }
            let limit: LocalAgentJSONValue?
            if name == "create_task" {
                limit = properties["final_response_max_chars"]
            } else {
                guard case .object(let tasks)? = properties["tasks"],
                      case .object(let items)? = tasks["items"],
                      case .object(let itemProperties)? = items["properties"] else { return XCTFail("missing item schema") }
                limit = itemProperties["final_response_max_chars"]
            }
            guard case .object(let constraint)? = limit else { return XCTFail("missing deterministic limit") }
            XCTAssertEqual(constraint["minimum"], .number(1))
            XCTAssertEqual(constraint["maximum"], .number(32_000))
        }
    }
    func testMainChatTaskSchemaFreezesInstalledPluginChoicesWithRoutingDescriptions() throws {
        let tools = NativeLocalAgentPlatformToolCatalog.capabilityTools(pluginChoices: [
            .init(
                pluginID: "plugin-2",
                pluginKey: NativeBrowserPluginIdentity.marketplaceKey,
                componentKey: NativeBrowserPluginIdentity.componentKey,
                displayName: "Browser CDP",
                description: "Browser pages."
            ),
            .init(
                pluginID: "plugin-1",
                pluginKey: "open-computer-use@chatos-marketplace",
                componentKey: "computer-use-mcp",
                displayName: "Computer Use",
                description: "Desktop control."
            ),
            .init(
                pluginID: "plugin-3",
                pluginKey: "rea@local",
                componentKey: "rea",
                displayName: "REA",
                description: "Reverse engineering."
            ),
        ])
        let create = try XCTUnwrap(tools.first { value in
            guard case .object(let tool) = value else { return false }
            return tool["name"] == .string("create_task")
        })
        guard case .object(let tool) = create,
              case .object(let parameters)? = tool["parameters"],
              case .object(let properties)? = parameters["properties"],
              case .object(let thinkingLevel)? = properties["thinking_level"],
              case .array(let thinkingLevels)? = thinkingLevel["enum"],
              case .object(let hints)? = properties["plugin_hints"],
              case .object(let items)? = hints["items"],
              case .array(let choices)? = items["oneOf"] else {
            return XCTFail("missing installed Plugin choice schema")
        }
        let identities = choices.compactMap { choice -> String? in
            guard case .object(let object) = choice,
                  case .object(let choiceProperties)? = object["properties"],
                  case .object(let pluginKey)? = choiceProperties["plugin_key"],
                  case .string(let key)? = pluginKey["const"],
                  case .object(let componentKey)? = choiceProperties["component_key"],
                  case .string(let component)? = componentKey["const"] else { return nil }
            return "\(key):\(component)"
        }
        XCTAssertEqual(identities, [
            "open-computer-use@chatos-marketplace:computer-use-mcp",
            "\(NativeBrowserPluginIdentity.marketplaceKey):\(NativeBrowserPluginIdentity.componentKey)",
            "rea@local:rea",
        ])
        XCTAssertEqual(thinkingLevels, [
            .string("none"), .string("auto"), .string("minimal"), .string("low"),
            .string("medium"), .string("high"), .string("xhigh"), .string("max"),
        ])
        let titles = choices.compactMap { choice -> String? in
            guard case .object(let object) = choice,
                  case .string(let title)? = object["title"] else { return nil }
            return title
        }
        XCTAssertTrue(titles.contains(where: {
            $0.contains("Browser pages.") && $0.contains("only for websites")
        }))
        XCTAssertTrue(titles.contains(where: {
            $0.contains("Desktop control.") && $0.contains("native desktop applications")
        }))
        guard case .string(let hintDescription)? = hints["description"] else {
            return XCTFail("missing plugin preservation guidance")
        }
        XCTAssertTrue(hintDescription.contains("explicitly names an installed Plugin or MCP"))
        XCTAssertTrue(hintDescription.contains("additive"))
        XCTAssertTrue(hintDescription.contains("must never replace"))
    }

    func testCustomInstalledPluginRemainsSelectableAlongsideManagedPlugins() {
        let managed = NativeInstalledAgentPlugin(
            pluginID: "browser-id",
            pluginKey: "browser@marketplace",
            componentKey: "browser-mcp",
            displayName: "Browser",
            description: "Browser automation"
        )
        let local = NativeInstalledAgentPlugin(
            pluginID: "rea-id",
            pluginKey: "rea@local",
            componentKey: "rea",
            displayName: "REA",
            description: "Reverse engineering"
        )
        let keys = NativeLocalConnectorService.selectablePluginKeys(
            managedKeys: [managed.pluginKey],
            installedPlugins: [managed, local],
            records: [
                managed.pluginID: .init(
                    pluginID: managed.pluginID, releaseID: "browser-release", version: "1.0.0",
                    artifactSHA256: String(repeating: "a", count: 64), installationPath: "/browser",
                    installedAt: "2026-10-17T00:00:00Z", pluginKey: managed.pluginKey,
                    source: .marketplace
                ),
                local.pluginID: .init(
                    pluginID: local.pluginID, releaseID: "rea-release", version: "6.5.0",
                    artifactSHA256: String(repeating: "b", count: 64), installationPath: "/rea",
                    installedAt: "2026-10-17T00:00:00Z", pluginKey: local.pluginKey,
                    source: .npmPlugin
                ),
                "not-installed": .init(
                    pluginID: "not-installed", releaseID: "unused", version: "1.0.0",
                    artifactSHA256: String(repeating: "c", count: 64), installationPath: "/unused",
                    installedAt: "2026-10-17T00:00:00Z", pluginKey: "unused@local",
                    source: .localPlugin
                ),
            ]
        )

        XCTAssertEqual(keys, ["browser@marketplace", "rea@local"])
    }

    func testTaskModelSchemaOnlyPublishesTaskEnabledChoices() throws {
        let tools = NativeLocalAgentPlatformToolCatalog.capabilityTools(
            pluginChoices: [],
            taskModelChoices: [
                .init(value: "task-model-2", title: "Task Model 2"),
                .init(value: "task-model-1", title: "Task Model 1"),
            ]
        )

        let createTask = try XCTUnwrap(tools.first { value in
            guard case .object(let tool) = value else { return false }
            return tool["name"] == .string("create_task")
        })
        guard case .object(let createTaskDefinition) = createTask,
              case .object(let createTaskParameters)? = createTaskDefinition["parameters"],
              case .object(let createTaskProperties)? = createTaskParameters["properties"],
              case .object(let createTaskModel)? = createTaskProperties["default_model_config_id"],
              case .array(let createTaskModels)? = createTaskModel["enum"] else {
            return XCTFail("missing create_task model choice schema")
        }
        XCTAssertEqual(createTaskModels, [.string("task-model-1"), .string("task-model-2")])

        let createTasks = try XCTUnwrap(tools.first { value in
            guard case .object(let tool) = value else { return false }
            return tool["name"] == .string("create_tasks_with_prerequisites")
        })
        guard case .object(let createTasksDefinition) = createTasks,
              case .object(let createTasksParameters)? = createTasksDefinition["parameters"],
              case .object(let createTasksProperties)? = createTasksParameters["properties"],
              case .object(let tasks)? = createTasksProperties["tasks"],
              case .object(let items)? = tasks["items"],
              case .object(let itemProperties)? = items["properties"],
              case .object(let createTasksModel)? = itemProperties["default_model_config_id"],
              case .array(let createTasksModels)? = createTasksModel["enum"] else {
            return XCTFail("missing batch Task model choice schema")
        }
        XCTAssertEqual(createTasksModels, createTaskModels)
    }

    func testTaskBuiltinSchemaDoesNotPresentNotepadAsGenericScratchStorage() throws {
        let tools = NativeLocalAgentPlatformToolCatalog.capabilityTools(
            pluginChoices: [],
            builtinChoices: [.init(value: "Notepad", title: "Notepad")]
        )
        let create = try XCTUnwrap(tools.first { value in
            guard case .object(let tool) = value else { return false }
            return tool["name"] == .string("create_task")
        })
        guard case .object(let definition) = create,
              case .object(let parameters)? = definition["parameters"],
              case .object(let properties)? = parameters["properties"],
              case .object(let builtins)? = properties["enabled_builtin_kinds"],
              case .object(let evidence)? = properties["notepad_user_request_quote"],
              case .string(let description)? = builtins["description"] else {
            return XCTFail("missing builtin capability routing description")
        }
        XCTAssertTrue(description.contains("explicitly requests a Notepad record"))
        XCTAssertTrue(description.contains("Never add a capability"))
        XCTAssertEqual(evidence["minLength"], .number(2))
        XCTAssertEqual(evidence["maxLength"], .number(500))
        guard case .string(let evidenceDescription)? = evidence["description"] else {
            return XCTFail("missing Notepad evidence description")
        }
        XCTAssertTrue(evidenceDescription.contains("exact phrase"))
    }

    func testMainChatTaskSchemaRequiresExplicitContractsAndRealDecomposition() throws {
        let tools = NativeLocalAgentPlatformToolCatalog.capabilityTools(pluginChoices: [])
        let createTask = try XCTUnwrap(tools.first { value in
            guard case .object(let tool) = value else { return false }
            return tool["name"] == .string("create_task")
        })
        guard case .object(let singleDefinition) = createTask,
              case .object(let singleParameters)? = singleDefinition["parameters"],
              case .array(let singleRequired)? = singleParameters["required"] else {
            return XCTFail("missing single Task required contract")
        }
        XCTAssertTrue(singleRequired.contains(.string("expected_outputs")))
        XCTAssertTrue(singleRequired.contains(.string("acceptance_criteria")))
        XCTAssertFalse(singleRequired.contains(.string("single_task_rationale")))

        let createTasks = try XCTUnwrap(tools.first { value in
            guard case .object(let tool) = value else { return false }
            return tool["name"] == .string("create_tasks_with_prerequisites")
        })
        guard case .object(let graphDefinition) = createTasks,
              case .object(let graphParameters)? = graphDefinition["parameters"],
              case .array(let graphRequired)? = graphParameters["required"],
              case .object(let graphProperties)? = graphParameters["properties"],
              case .object(let tasks)? = graphProperties["tasks"] else {
            return XCTFail("missing graph Task contract")
        }
        XCTAssertEqual(tasks["minItems"], LocalAgentJSONValue.number(2))
        XCTAssertTrue(graphRequired.contains(.string("graph_goal")))
        XCTAssertTrue(graphRequired.contains(.string("decomposition_rationale")))
        guard case .object(let notificationPolicy)? = graphProperties["notification_policy"],
              case .array(let notificationValues)? = notificationPolicy["enum"] else {
            return XCTFail("missing graph notification policy")
        }
        XCTAssertEqual(notificationPolicy["default"], .string("graph_summary"))
        XCTAssertEqual(notificationValues, [
            .string("all_tasks"), .string("milestones"), .string("graph_summary"),
        ])
        guard case .object(let taskItems)? = tasks["items"],
              case .object(let taskProperties)? = taskItems["properties"],
              case .object(let ownedPaths)? = taskProperties["owned_paths"],
              case .object(let ownedPathItems)? = ownedPaths["items"] else {
            return XCTFail("missing owned_paths safety contract")
        }
        XCTAssertEqual(ownedPaths["maxItems"], .number(200))
        guard case .object(let singleProperties)? = singleParameters["properties"] else {
            return XCTFail("missing single Task properties")
        }
        XCTAssertEqual(singleProperties["owned_paths"], taskProperties["owned_paths"])
        XCTAssertEqual(ownedPathItems["maxLength"], .number(4096))
        guard case .string(let ownershipDescription)? = ownedPaths["description"] else {
            return XCTFail("missing owned_paths description")
        }
        XCTAssertTrue(ownershipDescription.contains("whole project"))
        guard case .object(let milestone)? = taskProperties["notification_milestone"] else {
            return XCTFail("missing milestone notification marker")
        }
        XCTAssertEqual(milestone["type"], .string("boolean"))
        XCTAssertEqual(milestone["default"], .bool(false))
    }

    func testBootstrapPublishesVersionedOnDemandTaskSkillBundle() throws {
        let snapshot = try NativeLocalConnectorService.mainChatTaskSkillSnapshot()
        XCTAssertTrue(snapshot.checksum.hasPrefix("sha256:"))
        guard case .object(let message) = snapshot.prefixedInputItem,
              case .array(let content)? = message["content"],
              case .object(let textPart)? = content.first,
              case .string(let text)? = textPart["text"] else {
            return XCTFail("missing Task Skill system item")
        }
        XCTAssertTrue(text.contains("ChatOS Product Skill Catalog"))
        XCTAssertTrue(text.contains("product-skill:chatos-async-task-orchestration"))
        XCTAssertTrue(text.contains("agent_skill_activate"))
        XCTAssertFalse(text.contains("references/software-engineering.md"))

        let mainChatPolicy = try BundledAgentSkillLoader.load(named: "chatos-main-chat-invariants")
        XCTAssertTrue(mainChatPolicy.instructions.contains(
            "treat that as an execution request rather than a request for another plan summary"
        ))
        XCTAssertTrue(mainChatPolicy.instructions.contains(
            "Never end with a future-tense promise"
        ))
        XCTAssertTrue(mainChatPolicy.instructions.contains(
            "current-Run evidence proves that matching work is already active or was successfully created"
        ))

        XCTAssertEqual(snapshot.bundle.schemaVersion, 1)
        XCTAssertEqual(snapshot.bundle.bundleRevision, snapshot.checksum)
        XCTAssertEqual(Set(snapshot.bundle.documents.map(\.skillRef)), [
            "product-skill:chatos-async-task-orchestration",
            "product-skill:chatos-skill-runtime",
            "product-skill:chatos-main-chat-invariants",
            "product-skill:chatos-requirement-discovery",
        ])
        let taskBinding = try XCTUnwrap(snapshot.bundle.toolSkillBindings.first {
            $0.bindingID == ProductToolSkillBindingID.taskOrchestration
        })
        XCTAssertEqual(taskBinding.activationPolicy, .onDemand)
        XCTAssertEqual(Set(taskBinding.toolNames), Set(
            NativeLocalAgentPlatformToolCatalog.mainChatTaskToolNames
        ))
        let controlBinding = try XCTUnwrap(snapshot.bundle.toolSkillBindings.first {
            $0.bindingID == ProductToolSkillBindingID.agentSkillControlPlane
        })
        XCTAssertEqual(controlBinding.activationPolicy, .controlPlane)
        let surveyBinding = try XCTUnwrap(snapshot.bundle.toolSkillBindings.first {
            $0.bindingID == ProductToolSkillBindingID.requirementDiscovery
        })
        XCTAssertEqual(surveyBinding.activationPolicy, .onDemand)
        XCTAssertEqual(Set(surveyBinding.toolNames), Set(["requirement_survey_create", "requirement_plan_save"]))
        XCTAssertEqual(NativeLocalAgentPlatformToolCatalog.mainChatTaskToolNames.count, 8)
        XCTAssertEqual(NativeLocalAgentPlatformToolCatalog.capabilityTools.filter { tool in
            guard case let .object(value) = tool else { return false }
            return value["name"] == .string("requirement_survey_create")
        }.count, 1)

        let encoded = try JSONEncoder().encode(snapshot.bundle)
        XCTAssertLessThan(encoded.count, 256 * 1_024)
        let object = try XCTUnwrap(
            JSONSerialization.jsonObject(with: encoded) as? [String: Any]
        )
        XCTAssertEqual(object["schema_version"] as? Int, 1)
        XCTAssertNotNil(object["bundle_revision"])
        XCTAssertNotNil(object["tool_skill_bindings"])
    }

    func testBootstrapPublishesProfileSpecificTaskExecutionSkillBundle() throws {
        let snapshot = try NativeLocalConnectorService.taskExecutionSkillSnapshot()
        XCTAssertEqual(snapshot.bundle.schemaVersion, 2)
        XCTAssertTrue(snapshot.checksum.hasPrefix("sha256:"))
        guard case .object(let message) = snapshot.prefixedInputItem,
              case .array(let content)? = message["content"],
              case .object(let textPart)? = content.first,
              case .string(let text)? = textPart["text"] else {
            return XCTFail("missing Task Execution Skill catalog")
        }
        XCTAssertTrue(text.contains("ChatOS Task Execution Product Skill Catalog"))
        XCTAssertTrue(text.contains("product-skill:chatos-project-read"))
        XCTAssertTrue(text.contains("product-skill:chatos-terminal"))
        let discovery = try BundledAgentSkillLoader.load(named: "chatos-capability-discovery")
        XCTAssertTrue(text.contains(discovery.instructions))
        XCTAssertTrue(text.contains("direct_task_tool"))

        let bindings = snapshot.bundle.toolSkillBindings
        XCTAssertEqual(bindings.count, 6)
        XCTAssertEqual(
            Set(bindings.flatMap(\.toolNames)).count,
            bindings.flatMap(\.toolNames).count
        )
        XCTAssertTrue(bindings.flatMap(\.toolNames).allSatisfy(
            NativeLocalAgentPlatformToolCatalog.taskExecutionToolNames.contains
        ))
        let projectRead = try XCTUnwrap(bindings.first {
            $0.bindingID == ProductToolSkillBindingID.projectRead
        })
        XCTAssertEqual(projectRead.activationPolicy, .onDemand)
        XCTAssertTrue(projectRead.toolNames.contains("read_file"))
        let broker = try XCTUnwrap(bindings.first {
            $0.bindingID == ProductToolSkillBindingID.capabilityBroker
        })
        XCTAssertEqual(broker.activationPolicy, .runBound)
        XCTAssertTrue(broker.toolNames.contains("agent_skill_activate"))
        XCTAssertTrue(broker.toolNames.contains("capability_invoke"))
        XCTAssertTrue(Set(snapshot.bundle.documents.map(\.skillRef)).isSuperset(of: [
            "product-skill:chatos-capability-discovery",
            "product-skill:chatos-project-files",
            "product-skill:chatos-project-read",
            "product-skill:chatos-project-write",
            "product-skill:chatos-terminal",
        ]))
        XCTAssertLessThan(try JSONEncoder().encode(snapshot.bundle).count, 256 * 1_024)
    }

    func testProfileInstructionsComeFromExactlyOneFrozenPolicySkill() throws {
        let profiles = [
            ("chatos-main-chat-invariants", try NativeLocalConnectorService.mainChatTaskSkillSnapshot()),
            ("chatos-task-execution-invariants", try NativeLocalConnectorService.taskExecutionSkillSnapshot()),
        ]
        for (name, snapshot) in profiles {
            let policy = try XCTUnwrap(snapshot.bundle.documents.first { $0.name == name })
            XCTAssertEqual(policy.role, "policy")
            XCTAssertEqual(snapshot.instructions, policy.instructions)
            XCTAssertEqual(snapshot.instructions, try BundledAgentSkillLoader.load(named: name).instructions)
            XCTAssertEqual(policy.instructionsSHA256, NativeLocalConnectorService.sha256(snapshot.instructions))
            XCTAssertTrue(policy.resources.isEmpty)
            XCTAssertFalse(snapshot.bundle.toolSkillBindings.contains {
                $0.routerSkillRef == policy.skillRef || $0.specialistSkillRef == policy.skillRef
            }, "automatically loaded policy must not require on-demand activation")
            XCTAssertFalse(snapshot.instructions.contains("create_tasks_with_prerequisites"))
            XCTAssertFalse(snapshot.instructions.contains("wait_for_task_completion"))
        }
    }

    func testRenderedCatalogParticipatesInImmutableCapabilityRevision() throws {
        let main = try NativeLocalConnectorService.mainChatTaskSkillSnapshot()
        let task = try NativeLocalConnectorService.taskExecutionSkillSnapshot()
        let checksum = try NativeLocalConnectorService.combinedSkillChecksum(main: main, task: task)
        let changed = NativeMainChatTaskSkillSnapshot(
            checksum: main.checksum, instructions: main.instructions,
            prefixedInputItem: .object(["text": .string("changed catalog rendering")]),
            bundle: main.bundle
        )
        let changedChecksum = try NativeLocalConnectorService.combinedSkillChecksum(main: changed, task: task)
        XCTAssertNotEqual(checksum, changedChecksum)
        XCTAssertNotEqual(
            NativeLocalConnectorService.skillPolicyRevision(baseRevision: "policy-1", skillBundleChecksum: checksum),
            NativeLocalConnectorService.skillPolicyRevision(baseRevision: "policy-1", skillBundleChecksum: changedChecksum)
        )
        XCTAssertNotEqual(
            NativeLocalConnectorService.skillPolicyRevision(baseRevision: "policy-1", skillBundleChecksum: checksum),
            NativeLocalConnectorService.skillPolicyRevision(baseRevision: "policy-2", skillBundleChecksum: checksum)
        )
    }

    func testCapabilityRevisionFreezesActualSchemasAndRequiredPolicy() throws {
        func revision(
            mainTools: [LocalAgentJSONValue] = [.object(["name": .string("tool"), "description": .string("original")])],
            taskTools: [LocalAgentJSONValue] = [],
            policy: LocalAgentJSONValue = .object(["max_iterations": .number(600)])
        ) throws -> String {
            try NativeLocalConnectorService.capabilityRevision(
                baseRevision: "gateway-1", skillBundleChecksum: "bundle-1",
                mainTools: mainTools, taskTools: taskTools, taskPolicy: policy
            )
        }
        let base = try revision()
        XCTAssertEqual(base, "local-agent-7b9362e6c50fed7455504a444765a09897e50e0d95fd9e4ed80f43f6e64c8f33")
        XCTAssertEqual(base, try revision(), "identical publication must be replay-safe")
        XCTAssertNotEqual(base, try revision(mainTools: [.object([
            "name": .string("tool"), "description": .string("updated option description"),
        ])]))
        XCTAssertNotEqual(base, try revision(taskTools: [.object(["name": .string("external_mcp__new")])]))
        XCTAssertNotEqual(base, try revision(policy: .object(["max_iterations": .number(200)])))
        let reorderedKeys: LocalAgentJSONValue = .object(["description": .string("original"), "name": .string("tool")])
        XCTAssertEqual(base, try revision(mainTools: [reorderedKeys]))
    }
}
