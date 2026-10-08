@testable import ChatOSConnector
import XCTest

final class NativeLocalAgentPlatformToolCatalogTests: XCTestCase {
    func testMainChatTaskSchemaFreezesInstalledPluginChoicesWithRoutingDescriptions() throws {
        let tools = NativeLocalAgentPlatformToolCatalog.capabilityTools(pluginChoices: [
            .init(
                id: "plugin-2",
                pluginKey: NativeBrowserPluginIdentity.marketplaceKey,
                displayName: "Browser CDP",
                description: "Browser pages.",
                componentCount: 1
            ),
            .init(
                id: "plugin-1",
                pluginKey: "open-computer-use@chatos-marketplace",
                displayName: "Computer Use",
                description: "Desktop control.",
                componentCount: 1
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
              case .object(let hintProperties)? = items["properties"],
              case .object(let pluginKey)? = hintProperties["plugin_key"],
              case .array(let values)? = pluginKey["enum"],
              case .array(let choices)? = pluginKey["oneOf"] else {
            return XCTFail("missing installed Plugin choice schema")
        }
        XCTAssertEqual(values, [
            .string(NativeBrowserPluginIdentity.marketplaceKey),
            .string("open-computer-use@chatos-marketplace"),
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
}
