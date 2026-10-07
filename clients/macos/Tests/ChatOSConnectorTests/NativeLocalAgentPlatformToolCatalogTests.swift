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
}
