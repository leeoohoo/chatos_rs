import Foundation
import MCP

struct MCPService: Sendable {
    private let controller: ComputerController
    private let shortcuts: ShortcutCatalog

    init(
        controller: ComputerController = ComputerController(),
        shortcuts: ShortcutCatalog = ShortcutCatalog()
    ) {
        self.controller = controller
        self.shortcuts = shortcuts
    }

    func makeServer() async -> Server {
        let server = Server(
            name: "visual-computer-use",
            version: "0.8.11",
            title: "Visual Computer Use for macOS",
            instructions: "Treat screenshots as the source of UI truth; do not infer DOM or application internals. Every screenshot includes a visibly non-system AI orbit reticle and virtualCursorGlobal. When the cursor is inside the capture, cursorScreenshotPixel gives its top-left-origin image pixel coordinate; otherwise an edge indicator points toward it. After full-screen discovery, reuse the smallest recognizable global region for move_mouse and click to reduce capture and vision latency. Never skip the move_mouse or click image. key_press and type_text may set capture_after=false only for deterministic intermediate keyboard steps after the app and focus were verified; the next visible state change must still be observed. Read captureRegionGlobal, globalDesktopBounds, display frames, and globalPointsPerScreenshotPixelX/Y from every observation. move_mouse only animates the visible virtual reticle; click posts a real CoreGraphics click at its verified cyan center hotspot.",
            capabilities: .init(tools: .init(listChanged: false)),
            configuration: .strict
        )

        await server.withMethodHandler(ListTools.self) { _ in
            .init(tools: Self.tools)
        }

        await server.withMethodHandler(CallTool.self) { params in
            do {
                return try await call(
                    name: params.name,
                    arguments: params.arguments ?? [:]
                )
            } catch {
                return Self.failure(error)
            }
        }
        return server
    }

    private func call(name: String, arguments: [String: Value]) async throws -> CallTool.Result {
        switch name {
        case "check_permissions":
            let value = await controller.permissions()
            return try Self.jsonResult(value)

        case "request_permissions":
            let screenRecording = Self.bool(arguments, "screen_recording", default: true)
            let accessibility = Self.bool(arguments, "accessibility", default: true)
            let value = await controller.requestPermissions(
                screenRecording: screenRecording,
                accessibility: accessibility
            )
            return try Self.jsonResult(value)

        case "observe_screen":
            return try await captureResult(arguments)

        case "move_mouse":
            let x = try Self.number(arguments, "x")
            let y = try Self.number(arguments, "y")
            let target = PointDTO(x: x, y: y)
            try Self.requirePointInsideRequestedRegion(target, arguments: arguments)
            let duration = try Self.number(arguments, "duration", default: 1.2)
            let steps = try Self.int(arguments, "steps", default: 60)
            _ = try await controller.moveVirtualCursor(
                to: target,
                duration: duration,
                steps: steps
            )
            try await controller.wait(
                milliseconds: try Self.settleMilliseconds(arguments, default: 60)
            )
            return try await captureResult(arguments, defaultMaxWidth: 1400)

        case "click":
            try await requireCursorInsideRequestedRegion(arguments)
            let button = Self.string(arguments, "button") ?? "left"
            let count = try Self.int(arguments, "count", default: 1)
            let interval = try Self.number(arguments, "interval", default: 0.10)
            _ = try await controller.click(button: button, count: count, interval: interval)
            try await controller.wait(
                milliseconds: try Self.settleMilliseconds(arguments, default: 250)
            )
            return try await captureResult(arguments, defaultMaxWidth: 1400)

        case "scroll":
            try await requireCursorInsideRequestedRegion(arguments)
            let deltaX = try Self.validatedScrollDeltaPixels(
                Self.int32(arguments, "delta_x", default: 0),
                name: "delta_x"
            )
            let deltaY = try Self.validatedScrollDeltaPixels(
                Self.int32(arguments, "delta_y"),
                name: "delta_y"
            )
            let duration = try Self.number(arguments, "duration", default: 0.55)
            let steps = try Self.int(arguments, "steps", default: 18)
            try await controller.scroll(
                deltaX: deltaX,
                deltaY: deltaY,
                duration: duration,
                steps: steps
            )
            try await controller.wait(
                milliseconds: try Self.settleMilliseconds(arguments, default: 180)
            )
            return try await captureResult(arguments, defaultMaxWidth: 1400)

        case "type_text":
            guard let text = Self.string(arguments, "text") else {
                throw VisualComputerUseError.invalidArgument("text is required.")
            }
            try await controller.typeText(text)
            try await controller.wait(
                milliseconds: try Self.settleMilliseconds(arguments, default: 80)
            )
            return try await actionResult(
                "type_text",
                arguments: arguments,
                defaultMaxWidth: 1400
            )

        case "key_press":
            let keys = try Self.stringArray(arguments, "keys")
            try await controller.pressKeys(keys)
            try await controller.wait(
                milliseconds: try Self.settleMilliseconds(arguments, default: 120)
            )
            return try await actionResult(
                "key_press",
                arguments: arguments,
                defaultMaxWidth: 1400
            )

        case "active_application":
            return try await Self.jsonResult(controller.activeApplication())

        case "activate_application":
            guard let bundleIdentifier = Self.string(arguments, "bundle_identifier") else {
                throw VisualComputerUseError.invalidArgument(
                    "bundle_identifier is required."
                )
            }
            _ = try await controller.activateApplication(
                bundleIdentifier: bundleIdentifier
            )
            try await controller.wait(
                milliseconds: try Self.settleMilliseconds(arguments, default: 120)
            )
            return try await captureResult(arguments, defaultMaxWidth: 1400)

        case "list_shortcuts":
            let app = await controller.activeApplication()
            let query = Self.string(arguments, "query")
            let result = ShortcutListDTO(
                application: app,
                shortcuts: shortcuts.shortcuts(for: app, query: query),
                source: shortcuts.sourceDescription
            )
            return try Self.jsonResult(result)

        default:
            throw VisualComputerUseError.invalidArgument("Unknown tool '\(name)'.")
        }
    }

    private static let tools: [Tool] = [
        Tool(
            name: "check_permissions",
            title: "Check macOS permissions",
            description: "Check Screen Recording and Accessibility without prompting. Returns each permission's purpose, System Settings deep link, the exact app or executable macOS must authorize, whether a stable app-bundle identity is in use, and restart guidance.",
            inputSchema: objectSchema(),
            annotations: .init(readOnlyHint: true, openWorldHint: false)
        ),
        Tool(
            name: "request_permissions",
            title: "Request macOS permissions",
            description: "Show a native macOS onboarding window for Screen Recording and/or Accessibility. Each card explains why the permission is needed, opens the exact System Settings page, reveals the authorization target in Finder, polls for completion, and explains when the MCP must be reconnected.",
            inputSchema: objectSchema(properties: [
                "screen_recording": .object(["type": "boolean", "default": true]),
                "accessibility": .object(["type": "boolean", "default": true])
            ]),
            annotations: .init(readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false)
        ),
        Tool(
            name: "observe_screen",
            title: "Observe screen",
            description: "Capture a real display or global-point region. The first observation initializes and visibly renders the virtual cursor overlay. Every image returns virtualCursorGlobal and, when inside the capture, cursorScreenshotPixel in top-left-origin image pixels. Read-only crops may show an offscreen edge indicator.",
            inputSchema: objectSchema(properties: captureProperties()),
            annotations: .init(readOnlyHint: true, openWorldHint: false)
        ),
        Tool(
            name: "move_mouse",
            title: "Move mouse and observe",
            description: "Move only the server-rendered, non-system AI orbit reticle to a global point and return a screenshot. If region is provided, the target must be inside it so the returned crop contains the cyan center hotspot. This does not move the physical macOS cursor.",
            inputSchema: objectSchema(
                properties: mergedProperties([
                    "x": .object(["type": "number"]),
                    "y": .object(["type": "number"]),
                    "duration": .object(["type": "number", "default": 1.2, "minimum": 0, "maximum": 3, "description": "Visible virtual-pointer animation duration in seconds. The slower default makes the movement and trail easy for the user to follow."]),
                    "steps": .object(["type": "integer", "default": 60, "minimum": 2, "maximum": 80]),
                    "settle_ms": settleProperty(default: 60)
                ], captureProperties(defaultMaxWidth: 1400)),
                required: ["x", "y"]
            ),
            annotations: .init(readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: true)
        ),
        Tool(
            name: "click",
            title: "Click and observe",
            description: "Post a real CoreGraphics click at the cyan center of the current AI orbit reticle, wait for the UI, and return a screenshot. It accepts no x/y: call move_mouse and visually verify first.",
            inputSchema: objectSchema(properties: mergedProperties([
                "button": .object(["type": "string", "enum": ["left", "right", "middle"], "default": "left"]),
                "count": .object(["type": "integer", "minimum": 1, "maximum": 3, "default": 1]),
                "interval": .object(["type": "number", "minimum": 0, "maximum": 1, "default": 0.10]),
                "settle_ms": settleProperty(default: 250)
            ], captureProperties(defaultMaxWidth: 1400))),
            annotations: .init(readOnlyHint: false, destructiveHint: true, idempotentHint: false, openWorldHint: true)
        ),
        Tool(
            name: "scroll",
            title: "Scroll and observe",
            description: "Post a smooth sequence of pixel-based CoreGraphics scroll-wheel events at the virtual pointer, then return a fresh screenshot. Positive delta_y scrolls up; negative scrolls down. Use about 200-500 pixels for a normal scroll, observe the result, and repeat instead of attempting one large jump.",
            inputSchema: objectSchema(
                properties: mergedProperties([
                    "delta_y": .object([
                        "type": "integer",
                        "minimum": -1200,
                        "maximum": 1200,
                        "description": "Vertical distance in pixels. Positive scrolls up; negative scrolls down. Prefer 200-500 pixels per observation."
                    ]),
                    "delta_x": .object([
                        "type": "integer",
                        "minimum": -1200,
                        "maximum": 1200,
                        "default": 0,
                        "description": "Horizontal distance in pixels."
                    ]),
                    "duration": .object(["type": "number", "default": 0.55, "minimum": 0, "maximum": 3, "description": "Duration of the visible smooth scrolling sequence in seconds."]),
                    "steps": .object(["type": "integer", "default": 18, "minimum": 2, "maximum": 80, "description": "Number of eased scroll segments. The total requested delta is preserved exactly."]),
                    "settle_ms": settleProperty(default: 180)
                ], captureProperties(defaultMaxWidth: 1400)),
                required: ["delta_y"]
            ),
            annotations: .init(readOnlyHint: false, destructiveHint: false, idempotentHint: false, openWorldHint: true)
        ),
        Tool(
            name: "type_text",
            title: "Type text and observe",
            description: "Enter Unicode text through real CoreGraphics keyboard events without changing the clipboard, then optionally observe. This does not inspect or modify DOM, accessibility UI trees, or application internals.",
            inputSchema: objectSchema(
                properties: mergedProperties([
                    "text": .object(["type": "string"]),
                    "settle_ms": settleProperty(default: 80),
                    "capture_after": captureAfterProperty()
                ], captureProperties(defaultMaxWidth: 1400)),
                required: ["text"]
            ),
            annotations: .init(readOnlyHint: false, destructiveHint: true, idempotentHint: false, openWorldHint: true)
        ),
        Tool(
            name: "key_press",
            title: "Press shortcut and observe",
            description: "Press one real key with optional modifiers. Keep capture_after enabled for navigation or visible state changes; set it to false only for deterministic intermediate chords such as select-all or copy when the app and focus were just verified.",
            inputSchema: objectSchema(
                properties: mergedProperties([
                    "keys": .object([
                        "type": "array",
                        "items": .object(["type": "string"]),
                        "minItems": 1
                    ]),
                    "settle_ms": settleProperty(default: 120),
                    "capture_after": captureAfterProperty()
                ], captureProperties(defaultMaxWidth: 1400)),
                required: ["keys"]
            ),
            annotations: .init(readOnlyHint: false, destructiveHint: true, idempotentHint: false, openWorldHint: true)
        ),
        Tool(
            name: "active_application",
            title: "Get active application",
            description: "Return public process metadata for the frontmost application without inspecting its UI tree or internal state.",
            inputSchema: objectSchema(),
            annotations: .init(readOnlyHint: true, openWorldHint: false)
        ),
        Tool(
            name: "activate_application",
            title: "Activate application and observe",
            description: "Launch or activate an application through public NSWorkspace APIs, then return a real screenshot. Example bundle id: com.apple.Notes.",
            inputSchema: objectSchema(
                properties: mergedProperties([
                    "bundle_identifier": .object(["type": "string"]),
                    "settle_ms": settleProperty(default: 120)
                ], captureProperties(defaultMaxWidth: 1400)),
                required: ["bundle_identifier"]
            ),
            annotations: .init(readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: true)
        ),
        Tool(
            name: "list_shortcuts",
            title: "List known shortcuts",
            description: "List public or user-configured shortcuts for the frontmost application. Execute the returned keys with key_press.",
            inputSchema: objectSchema(properties: [
                "query": .object(["type": "string", "description": "Optional id, title, description, or key filter."])
            ]),
            annotations: .init(readOnlyHint: true, openWorldHint: false)
        )
    ]

    private func captureResult(
        _ arguments: [String: Value],
        defaultMaxWidth: Int = 1600
    ) async throws -> CallTool.Result {
        let observation = try await controller.observe(
            options: try Self.captureOptions(
                arguments,
                defaultMaxWidth: defaultMaxWidth
            )
        )
        return try Self.observationResult(observation)
    }

    private func actionResult(
        _ action: String,
        arguments: [String: Value],
        defaultMaxWidth: Int
    ) async throws -> CallTool.Result {
        if Self.bool(arguments, "capture_after", default: true) {
            return try await captureResult(
                arguments,
                defaultMaxWidth: defaultMaxWidth
            )
        }
        let receipt = ActionReceiptDTO(
            action: action,
            screenshotReturned: false,
            virtualCursorGlobal: try await controller.currentVirtualCursor(),
            activeApplication: await controller.activeApplication()
        )
        return try Self.jsonResult(receipt)
    }

    private func requireCursorInsideRequestedRegion(
        _ arguments: [String: Value]
    ) async throws {
        guard let region = try Self.requestedRegion(arguments) else { return }
        let cursor = try await controller.currentVirtualCursor()
        guard ComputerController.contains(region, cursor) else {
            throw VisualComputerUseError.invalidArgument(
                "The requested partial region does not contain the virtual cursor at (\(cursor.x), \(cursor.y)). Call move_mouse with a target inside the region before this action."
            )
        }
    }

    private static func requirePointInsideRequestedRegion(
        _ point: PointDTO,
        arguments: [String: Value]
    ) throws {
        guard let region = try requestedRegion(arguments) else { return }
        guard ComputerController.contains(region, point) else {
            throw VisualComputerUseError.invalidArgument(
                "move_mouse target (\(point.x), \(point.y)) must be inside the requested partial region."
            )
        }
    }

    private static func captureOptions(
        _ arguments: [String: Value],
        defaultMaxWidth: Int
    ) throws -> ComputerController.ObservationOptions {
        let displayID = try optionalUInt32(arguments, "display_id")
        let maxImageWidth = try int(
            arguments,
            "max_image_width",
            default: defaultMaxWidth
        )
        guard maxImageWidth == 0 || (320...8192).contains(maxImageWidth) else {
            throw VisualComputerUseError.invalidArgument(
                "max_image_width must be 0 or an integer from 320 through 8192."
            )
        }

        let formatName = (string(arguments, "image_format") ?? "jpeg").lowercased()
        guard let format = ComputerController.ScreenshotFormat(rawValue: formatName) else {
            throw VisualComputerUseError.invalidArgument(
                "image_format must be png or jpeg."
            )
        }

        let jpegQuality = try number(arguments, "jpeg_quality", default: 0.82)
        guard (0.1...1).contains(jpegQuality) else {
            throw VisualComputerUseError.invalidArgument(
                "jpeg_quality must be between 0.1 and 1.0."
            )
        }

        let region = try requestedRegion(arguments)

        return ComputerController.ObservationOptions(
            displayID: displayID,
            region: region,
            maxImageWidth: maxImageWidth,
            format: format,
            jpegQuality: jpegQuality
        )
    }

    private static func requestedRegion(
        _ arguments: [String: Value]
    ) throws -> RectDTO? {
        if let rawRegion = arguments["region"] {
            guard let object = rawRegion.objectValue else {
                throw VisualComputerUseError.invalidArgument(
                    "region must be an object with x, y, width, and height."
                )
            }
            return RectDTO(
                x: try number(object, "x"),
                y: try number(object, "y"),
                width: try number(object, "width"),
                height: try number(object, "height")
            )
        }
        return nil
    }

    private static func captureProperties(
        defaultMaxWidth: Int = 1600
    ) -> [String: Value] {
        [
            "display_id": .object([
                "type": "integer",
                "minimum": 0,
                "description": "Optional CGDirectDisplayID. If omitted, use the display containing region or the cursor."
            ]),
            "region": .object([
                "type": "object",
                "description": "Optional capture rectangle in global macOS point coordinates. It must fit within one display.",
                "properties": .object([
                    "x": .object(["type": "number"]),
                    "y": .object(["type": "number"]),
                    "width": .object(["type": "number", "exclusiveMinimum": 0]),
                    "height": .object(["type": "number", "exclusiveMinimum": 0])
                ]),
                "required": .array(["x", "y", "width", "height"].map(Value.string)),
                "additionalProperties": false
            ]),
            "max_image_width": .object([
                "type": "integer",
                "anyOf": .array([
                    .object(["const": 0]),
                    .object(["minimum": 320, "maximum": 8192])
                ]),
                "default": .int(defaultMaxWidth),
                "description": "Maximum encoded width. Use 0 for native capture width; nonzero values must be at least 320."
            ]),
            "image_format": .object([
                "type": "string",
                "enum": .array(["jpeg", "png"].map(Value.string)),
                "default": "jpeg"
            ]),
            "jpeg_quality": .object([
                "type": "number",
                "minimum": 0.1,
                "maximum": 1,
                "default": 0.82,
                "description": "JPEG quality; ignored when image_format is png."
            ])
        ]
    }

    private static func mergedProperties(
        _ first: [String: Value],
        _ second: [String: Value]
    ) -> [String: Value] {
        first.merging(second) { current, _ in current }
    }

    private static func settleProperty(default defaultValue: Int) -> Value {
        .object([
            "type": "integer",
            "minimum": 0,
            "maximum": 5000,
            "default": .int(defaultValue),
            "description": "Delay after the input event before capturing the result."
        ])
    }

    private static func captureAfterProperty() -> Value {
        .object([
            "type": "boolean",
            "default": true,
            "description": "Return the usual post-action screenshot. Set false only for a deterministic intermediate keyboard step when focus and the active app were just visually verified; the next state-changing step must still be observed."
        ])
    }

    private static func settleMilliseconds(
        _ arguments: [String: Value],
        default defaultValue: Int
    ) throws -> Int {
        let milliseconds = try int(arguments, "settle_ms", default: defaultValue)
        guard (0...5000).contains(milliseconds) else {
            throw VisualComputerUseError.invalidArgument(
                "settle_ms must be an integer from 0 through 5000."
            )
        }
        return milliseconds
    }

    static func validatedScrollDeltaPixels(
        _ value: Int32,
        name: String
    ) throws -> Int32 {
        let limit: Int32 = 1_200
        guard value >= -limit, value <= limit else {
            throw VisualComputerUseError.invalidArgument(
                "\(name) must be an integer from -1200 through 1200 pixels."
            )
        }
        return value
    }

    private static func observationResult(
        _ observation: ComputerController.CapturedObservation
    ) throws -> CallTool.Result {
        let cursor = observation.metadata.virtualCursorGlobal
        let region = observation.metadata.captureRegionGlobal
        let imagePixel = observation.metadata.cursorScreenshotPixel.map {
            "(\($0.x), \($0.y))"
        } ?? "outside capture"
        let application = observation.metadata.activeApplication
        let applicationName = application.name ?? "unknown"
        let bundleIdentifier = application.bundleIdentifier ?? "unknown"
        let summary = "Active application: \(applicationName) (\(bundleIdentifier)). Virtual mouse global coordinate: (\(cursor.x), \(cursor.y)). Cursor screenshot pixel: \(imagePixel). Screenshot global region: x=\(region.x), y=\(region.y), width=\(region.width), height=\(region.height). Global points per screenshot pixel: x=\(observation.metadata.globalPointsPerScreenshotPixelX), y=\(observation.metadata.globalPointsPerScreenshotPixelY). Cursor visualization: \(observation.metadata.cursorVisualization). Exact metadata is in structuredContent."
        return CallTool.Result(
            content: [
                .text(
                    text: summary,
                    annotations: nil,
                    _meta: nil
                ),
                .image(
                    data: observation.imageData.base64EncodedString(),
                    mimeType: observation.mimeType,
                    annotations: nil,
                    _meta: nil
                )
            ],
            structuredContent: Optional.some(try Value(observation.metadata)),
            isError: false
        )
    }

    private static func objectSchema(
        properties: [String: Value] = [:],
        required: [String] = []
    ) -> Value {
        var schema: [String: Value] = [
            "type": "object",
            "properties": .object(properties),
            "additionalProperties": false
        ]
        if !required.isEmpty {
            schema["required"] = .array(required.map(Value.string))
        }
        return .object(schema)
    }

    private static func bool(_ arguments: [String: Value], _ key: String, default fallback: Bool) -> Bool {
        arguments[key]?.boolValue ?? fallback
    }

    private static func string(_ arguments: [String: Value], _ key: String) -> String? {
        arguments[key]?.stringValue
    }

    private static func number(
        _ arguments: [String: Value],
        _ key: String,
        default fallback: Double? = nil
    ) throws -> Double {
        if let value = arguments[key] {
            if let double = value.doubleValue { return double }
            if let int = value.intValue { return Double(int) }
        }
        if let fallback { return fallback }
        throw VisualComputerUseError.invalidArgument("\(key) must be a number.")
    }

    private static func int(
        _ arguments: [String: Value],
        _ key: String,
        default fallback: Int? = nil
    ) throws -> Int {
        if let value = arguments[key]?.intValue { return value }
        if let fallback { return fallback }
        throw VisualComputerUseError.invalidArgument("\(key) must be an integer.")
    }

    private static func int32(
        _ arguments: [String: Value],
        _ key: String,
        default fallback: Int32? = nil
    ) throws -> Int32 {
        let value: Int
        if let int = arguments[key]?.intValue {
            value = int
        } else if let fallback {
            return fallback
        } else {
            throw VisualComputerUseError.invalidArgument("\(key) must be an integer.")
        }
        guard let converted = Int32(exactly: value) else {
            throw VisualComputerUseError.invalidArgument("\(key) is outside the Int32 range.")
        }
        return converted
    }

    private static func optionalUInt32(
        _ arguments: [String: Value],
        _ key: String
    ) throws -> UInt32? {
        guard let raw = arguments[key] else { return nil }
        guard let int = raw.intValue, let value = UInt32(exactly: int) else {
            throw VisualComputerUseError.invalidArgument("\(key) must be a UInt32 integer.")
        }
        return value
    }

    private static func stringArray(_ arguments: [String: Value], _ key: String) throws -> [String] {
        guard let values = arguments[key]?.arrayValue else {
            throw VisualComputerUseError.invalidArgument("\(key) must be an array of strings.")
        }
        let strings = values.compactMap(\.stringValue)
        guard strings.count == values.count else {
            throw VisualComputerUseError.invalidArgument("\(key) must contain only strings.")
        }
        return strings
    }

    private static func jsonResult<T: Codable>(_ value: T) throws -> CallTool.Result {
        CallTool.Result(
            content: [.text(text: try jsonString(value), annotations: nil, _meta: nil)],
            structuredContent: Optional.some(try Value(value)),
            isError: false
        )
    }

    private static func failure(_ error: Error) -> CallTool.Result {
        .init(
            content: [.text(text: error.localizedDescription, annotations: nil, _meta: nil)],
            isError: true
        )
    }

    private static func jsonString<T: Encodable>(_ value: T) throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        return String(decoding: try encoder.encode(value), as: UTF8.self)
    }
}
