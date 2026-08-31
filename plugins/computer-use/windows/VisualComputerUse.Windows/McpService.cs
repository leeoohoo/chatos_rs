using System.Text.Json;
using System.Text.Json.Nodes;

namespace VisualComputerUse.Windows;

internal sealed class McpService(ComputerController controller)
{
    internal const string Name = "visual-computer-use";
    internal const string Version = "0.9.0";

    internal static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = false
    };

    private readonly SemaphoreSlim actionGate = new(1, 1);

    internal JsonObject Initialize(JsonObject? parameters)
    {
        var protocolVersion = parameters?["protocolVersion"]?.GetValue<string>() ?? "2025-06-18";
        return new JsonObject
        {
            ["protocolVersion"] = protocolVersion,
            ["capabilities"] = new JsonObject
            {
                ["tools"] = new JsonObject { ["listChanged"] = false }
            },
            ["serverInfo"] = new JsonObject
            {
                ["name"] = Name,
                ["version"] = Version,
                ["title"] = "Visual Computer Use for Windows"
            },
            ["instructions"] = "Treat real screenshots as the source of UI truth; do not infer DOM, Accessibility/UI Automation trees, or application internals. Every screenshot contains a visibly non-system AI orbit reticle and global coordinates. After full-screen discovery, reuse the smallest recognizable global region for move_mouse and click. Never skip their images. key_press and type_text may set capture_after=false only for deterministic intermediate keyboard steps after focus and the foreground app were verified; the next visible state change must still be observed. click and scroll target the cyan center at virtualCursorGlobal, briefly move the physical pointer there, send real SendInput events, and restore the user's original pointer position."
        };
    }

    internal JsonObject ListTools() => new() { ["tools"] = BuildTools() };

    internal async Task<JsonObject> CallToolAsync(string name, JsonObject arguments)
    {
        await actionGate.WaitAsync().ConfigureAwait(false);
        try
        {
            return await CallToolCoreAsync(name, arguments).ConfigureAwait(false);
        }
        catch (Exception error)
        {
            return Failure(error.Message);
        }
        finally
        {
            actionGate.Release();
        }
    }

    private async Task<JsonObject> CallToolCoreAsync(string name, JsonObject arguments)
    {
        switch (name)
        {
            case "check_permissions":
                return JsonResult(controller.CheckPermissions());
            case "request_permissions":
                return JsonResult(await controller.RequestPermissionsAsync().ConfigureAwait(false));
            case "observe_screen":
                return await CaptureResultAsync(arguments, 1600).ConfigureAwait(false);
            case "move_mouse":
            {
                var target = new PointDto(Number(arguments, "x"), Number(arguments, "y"));
                RequirePointInsideRequestedRegion(target, arguments);
                await controller.MoveVirtualCursorAsync(
                    target,
                    Number(arguments, "duration", 1.2),
                    Integer(arguments, "steps", 60)).ConfigureAwait(false);
                await SettleAsync(arguments, 60).ConfigureAwait(false);
                return await CaptureResultAsync(arguments, 1400).ConfigureAwait(false);
            }
            case "click":
                RequireCursorInsideRequestedRegion(arguments);
                await controller.ClickAsync(
                    String(arguments, "button") ?? "left",
                    Integer(arguments, "count", 1),
                    Number(arguments, "interval", 0.10)).ConfigureAwait(false);
                await SettleAsync(arguments, 250).ConfigureAwait(false);
                return await CaptureResultAsync(arguments, 1400).ConfigureAwait(false);
            case "scroll":
                RequireCursorInsideRequestedRegion(arguments);
                await controller.ScrollAsync(
                    Integer(arguments, "delta_x", 0),
                    Integer(arguments, "delta_y"),
                    Number(arguments, "duration", 0.55),
                    Integer(arguments, "steps", 18)).ConfigureAwait(false);
                await SettleAsync(arguments, 180).ConfigureAwait(false);
                return await CaptureResultAsync(arguments, 1400).ConfigureAwait(false);
            case "type_text":
                controller.TypeText(String(arguments, "text")
                    ?? throw new VisualComputerUseException("text is required."));
                await SettleAsync(arguments, 80).ConfigureAwait(false);
                return await ActionResultAsync("type_text", arguments, 1400).ConfigureAwait(false);
            case "key_press":
                controller.PressKeys(StringArray(arguments, "keys"));
                await SettleAsync(arguments, 120).ConfigureAwait(false);
                return await ActionResultAsync("key_press", arguments, 1400).ConfigureAwait(false);
            case "active_application":
                return JsonResult(controller.ActiveApplication());
            case "activate_application":
                await controller.ActivateApplicationAsync(String(arguments, "application")
                    ?? throw new VisualComputerUseException("application is required.")).ConfigureAwait(false);
                await SettleAsync(arguments, 350).ConfigureAwait(false);
                return await CaptureResultAsync(arguments, 1400).ConfigureAwait(false);
            case "list_shortcuts":
                return JsonResult(ShortcutCatalog.List(
                    controller.ActiveApplication(),
                    String(arguments, "query")));
            default:
                throw new VisualComputerUseException($"Unknown tool '{name}'.");
        }
    }

    private async Task<JsonObject> CaptureResultAsync(JsonObject arguments, int defaultMaxWidth)
    {
        var observation = await controller.ObserveAsync(CaptureOptions(arguments, defaultMaxWidth)).ConfigureAwait(false);
        var metadata = JsonSerializer.SerializeToNode(observation.Metadata, JsonOptions)
            ?? throw new VisualComputerUseException("Could not serialize observation metadata.");
        var cursor = observation.Metadata.VirtualCursorGlobal;
        var region = observation.Metadata.CaptureRegionGlobal;
        var imagePixel = observation.Metadata.CursorScreenshotPixel is null
            ? "outside capture"
            : $"({observation.Metadata.CursorScreenshotPixel.X}, {observation.Metadata.CursorScreenshotPixel.Y})";
        var application = observation.Metadata.ActiveApplication;
        var summary = $"Active application: {application.Name ?? "unknown"} ({application.Executable ?? "unknown"}). Virtual mouse global coordinate: ({cursor.X}, {cursor.Y}). Cursor screenshot pixel: {imagePixel}. Screenshot global region: x={region.X}, y={region.Y}, width={region.Width}, height={region.Height}. Global points per screenshot pixel: x={observation.Metadata.GlobalPointsPerScreenshotPixelX}, y={observation.Metadata.GlobalPointsPerScreenshotPixelY}. Cursor visualization: {observation.Metadata.CursorVisualization}. Exact metadata is in structuredContent.";
        return new JsonObject
        {
            ["content"] = new JsonArray
            {
                new JsonObject { ["type"] = "text", ["text"] = summary },
                new JsonObject
                {
                    ["type"] = "image",
                    ["data"] = Convert.ToBase64String(observation.ImageData),
                    ["mimeType"] = observation.MimeType
                }
            },
            ["structuredContent"] = metadata.DeepClone(),
            ["isError"] = false
        };
    }

    private async Task<JsonObject> ActionResultAsync(
        string action,
        JsonObject arguments,
        int defaultMaxWidth)
    {
        if (Boolean(arguments, "capture_after", true))
            return await CaptureResultAsync(arguments, defaultMaxWidth).ConfigureAwait(false);
        return JsonResult(new
        {
            action,
            screenshotReturned = false,
            virtualCursorGlobal = controller.CurrentVirtualCursor(),
            activeApplication = controller.ActiveApplication()
        });
    }

    private static ObservationOptions CaptureOptions(JsonObject arguments, int defaultMaxWidth)
    {
        var maxWidth = Integer(arguments, "max_image_width", defaultMaxWidth);
        if (maxWidth != 0 && (maxWidth < 320 || maxWidth > 8192))
            throw new VisualComputerUseException("max_image_width must be 0 or an integer from 320 through 8192.");
        var quality = Number(arguments, "jpeg_quality", 0.82);
        if (quality is < 0.1 or > 1)
            throw new VisualComputerUseException("jpeg_quality must be between 0.1 and 1.0.");
        var formatName = (String(arguments, "image_format") ?? "jpeg").ToLowerInvariant();
        var format = formatName switch
        {
            "jpeg" => ScreenshotFormat.Jpeg,
            "png" => ScreenshotFormat.Png,
            _ => throw new VisualComputerUseException("image_format must be png or jpeg.")
        };
        return new ObservationOptions(
            String(arguments, "display_id"),
            RequestedRegion(arguments),
            maxWidth,
            format,
            quality);
    }

    private void RequireCursorInsideRequestedRegion(JsonObject arguments)
    {
        var region = RequestedRegion(arguments);
        if (region is null)
            return;
        var cursor = controller.CurrentVirtualCursor();
        if (!region.Contains(cursor))
            throw new VisualComputerUseException($"The requested partial region does not contain the virtual cursor at ({cursor.X}, {cursor.Y}). Call move_mouse with a target inside the region before this action.");
    }

    private static void RequirePointInsideRequestedRegion(PointDto point, JsonObject arguments)
    {
        var region = RequestedRegion(arguments);
        if (region is not null && !region.Contains(point))
            throw new VisualComputerUseException($"move_mouse target ({point.X}, {point.Y}) must be inside the requested partial region.");
    }

    private static RectDto? RequestedRegion(JsonObject arguments)
    {
        if (arguments["region"] is null)
            return null;
        if (arguments["region"] is not JsonObject region)
            throw new VisualComputerUseException("region must be an object with x, y, width, and height.");
        var result = new RectDto(
            Number(region, "x"),
            Number(region, "y"),
            Number(region, "width"),
            Number(region, "height"));
        if (result.Width <= 0 || result.Height <= 0)
            throw new VisualComputerUseException("region width and height must be greater than zero.");
        return result;
    }

    private static async Task SettleAsync(JsonObject arguments, int fallback)
    {
        var milliseconds = Integer(arguments, "settle_ms", fallback);
        if (milliseconds is < 0 or > 5000)
            throw new VisualComputerUseException("settle_ms must be an integer from 0 through 5000.");
        if (milliseconds > 0)
            await Task.Delay(milliseconds).ConfigureAwait(false);
    }

    private static string? String(JsonObject arguments, string key) =>
        arguments[key] is JsonValue value && value.TryGetValue<string>(out var result) ? result : null;

    private static bool Boolean(JsonObject arguments, string key, bool fallback) =>
        arguments[key] is JsonValue value && value.TryGetValue<bool>(out var result) ? result : fallback;

    private static double Number(JsonObject arguments, string key, double? fallback = null)
    {
        if (arguments[key] is JsonValue value)
        {
            if (value.TryGetValue<double>(out var number))
                return number;
            if (value.TryGetValue<int>(out var integer))
                return integer;
        }
        return fallback ?? throw new VisualComputerUseException($"{key} must be a number.");
    }

    private static int Integer(JsonObject arguments, string key, int? fallback = null)
    {
        if (arguments[key] is JsonValue value && value.TryGetValue<int>(out var result))
            return result;
        return fallback ?? throw new VisualComputerUseException($"{key} must be an integer.");
    }

    private static IReadOnlyList<string> StringArray(JsonObject arguments, string key)
    {
        if (arguments[key] is not JsonArray values || values.Count == 0)
            throw new VisualComputerUseException($"{key} must be a non-empty array of strings.");
        var result = values.Select(value => value?.GetValue<string>()
            ?? throw new VisualComputerUseException($"{key} must contain only strings.")).ToArray();
        return result;
    }

    private static JsonObject JsonResult<T>(T value)
    {
        var node = JsonSerializer.SerializeToNode(value, JsonOptions)
            ?? throw new VisualComputerUseException("Could not serialize tool result.");
        return new JsonObject
        {
            ["content"] = new JsonArray
            {
                new JsonObject { ["type"] = "text", ["text"] = node.ToJsonString(JsonOptions) }
            },
            ["structuredContent"] = node.DeepClone(),
            ["isError"] = false
        };
    }

    private static JsonObject Failure(string message) => new()
    {
        ["content"] = new JsonArray
        {
            new JsonObject { ["type"] = "text", ["text"] = message }
        },
        ["isError"] = true
    };

    private static JsonArray BuildTools() =>
    [
        Tool("check_permissions", "Check Windows capabilities", "Check interactive-desktop, capture, SendInput, elevation, and Windows platform limitations without prompting.", ObjectSchema(), true, false, true, false),
        Tool("request_permissions", "Show Windows capability guide", "Show a native Windows guide explaining interactive desktop availability, Administrator/UIPI boundaries, secure desktop limitations, and the exact executable in use.", ObjectSchema(), false, false, true, false),
        Tool("observe_screen", "Observe screen", "Capture a real Windows display or global-pixel region. Every image contains the virtual cursor, global coordinate metadata, and an offscreen indicator when needed.", ObjectSchema(CaptureProperties(1600)), true, false, true, false),
        Tool("move_mouse", "Move mouse and observe", "Animate only the server-rendered, non-system AI orbit reticle to a Windows global pixel coordinate and return a screenshot. This does not move the physical Windows cursor.", ObjectSchema(Merge(
            new JsonObject
            {
                ["x"] = NumberProperty(), ["y"] = NumberProperty(),
                ["duration"] = NumberProperty(1.2, 0, 3, "Visible animation duration in seconds."),
                ["steps"] = IntegerProperty(60, 2, 80),
                ["settle_ms"] = SettleProperty(60)
            }, CaptureProperties(1400)), ["x", "y"]), false, false, true, true),
        Tool("click", "Click and observe", "Send a real Windows click at the visually verified cyan center of the AI reticle, restore the user's physical pointer position, and return a screenshot. It accepts no x/y; call move_mouse first.", ObjectSchema(Merge(
            new JsonObject
            {
                ["button"] = EnumProperty(["left", "right", "middle"], "left"),
                ["count"] = IntegerProperty(1, 1, 3),
                ["interval"] = NumberProperty(0.10, 0, 1),
                ["settle_ms"] = SettleProperty(250)
            }, CaptureProperties(1400))), false, true, false, true),
        Tool("scroll", "Scroll and observe", "Send a smooth sequence of real Windows wheel events at the virtual pointer, restore the physical pointer, and observe. Positive delta_y scrolls up; negative scrolls down.", ObjectSchema(Merge(
            new JsonObject
            {
                ["delta_y"] = IntegerProperty(), ["delta_x"] = IntegerProperty(0),
                ["duration"] = NumberProperty(0.55, 0, 3), ["steps"] = IntegerProperty(18, 2, 80),
                ["settle_ms"] = SettleProperty(180)
            }, CaptureProperties(1400)), ["delta_y"]), false, false, false, true),
        Tool("type_text", "Type text and observe", "Enter text through real Windows KEYEVENTF_UNICODE SendInput events without changing the clipboard, then optionally observe.", ObjectSchema(Merge(
            new JsonObject
            {
                ["text"] = new JsonObject { ["type"] = "string" },
                ["settle_ms"] = SettleProperty(80),
                ["capture_after"] = CaptureAfterProperty()
            },
            CaptureProperties(1400)), ["text"]), false, true, false, true),
        Tool("key_press", "Press shortcut and observe", "Press one real Windows key with optional modifiers using SendInput. Disable capture_after only for deterministic intermediate chords after focus and foreground-app verification.", ObjectSchema(Merge(
            new JsonObject
            {
                ["keys"] = new JsonObject { ["type"] = "array", ["items"] = new JsonObject { ["type"] = "string" }, ["minItems"] = 1 },
                ["settle_ms"] = SettleProperty(120),
                ["capture_after"] = CaptureAfterProperty()
            }, CaptureProperties(1400)), ["keys"]), false, true, false, true),
        Tool("active_application", "Get active application", "Return public foreground process and window metadata without inspecting the application's UI tree or internal state.", ObjectSchema(), true, false, true, false),
        Tool("activate_application", "Activate application and observe", "Activate an existing process by name or launch an executable, shell application name, or path, then return a real screenshot.", ObjectSchema(Merge(
            new JsonObject { ["application"] = new JsonObject { ["type"] = "string" }, ["settle_ms"] = SettleProperty(350) },
            CaptureProperties(1400)), ["application"]), false, false, true, true),
        Tool("list_shortcuts", "List known shortcuts", "List public built-in Windows shortcuts for the foreground application. Execute returned keys with key_press.", ObjectSchema(new JsonObject
        {
            ["query"] = new JsonObject { ["type"] = "string", ["description"] = "Optional shortcut filter." }
        }), true, false, true, false)
    ];

    private static JsonObject Tool(
        string name,
        string title,
        string description,
        JsonObject inputSchema,
        bool readOnly,
        bool destructive,
        bool idempotent,
        bool openWorld) => new()
    {
        ["name"] = name,
        ["title"] = title,
        ["description"] = description,
        ["inputSchema"] = inputSchema,
        ["annotations"] = new JsonObject
        {
            ["readOnlyHint"] = readOnly,
            ["destructiveHint"] = destructive,
            ["idempotentHint"] = idempotent,
            ["openWorldHint"] = openWorld
        }
    };

    private static JsonObject ObjectSchema(JsonObject? properties = null, string[]? required = null)
    {
        var schema = new JsonObject
        {
            ["type"] = "object",
            ["properties"] = properties ?? new JsonObject(),
            ["additionalProperties"] = false
        };
        if (required is { Length: > 0 })
            schema["required"] = new JsonArray(required.Select(value => JsonValue.Create(value)).ToArray());
        return schema;
    }

    private static JsonObject CaptureProperties(int defaultMaxWidth) => new()
    {
        ["display_id"] = new JsonObject
        {
            ["type"] = "string",
            ["description"] = "Optional display id returned by observe_screen. If omitted, use the display containing region or the cursor."
        },
        ["region"] = new JsonObject
        {
            ["type"] = "object",
            ["description"] = "Optional rectangle in global Windows physical pixels; it must fit within one display.",
            ["properties"] = new JsonObject
            {
                ["x"] = NumberProperty(), ["y"] = NumberProperty(),
                ["width"] = new JsonObject { ["type"] = "number", ["exclusiveMinimum"] = 0 },
                ["height"] = new JsonObject { ["type"] = "number", ["exclusiveMinimum"] = 0 }
            },
            ["required"] = new JsonArray(
                JsonValue.Create("x"),
                JsonValue.Create("y"),
                JsonValue.Create("width"),
                JsonValue.Create("height")),
            ["additionalProperties"] = false
        },
        ["max_image_width"] = new JsonObject
        {
            ["type"] = "integer",
            ["anyOf"] = new JsonArray
            {
                new JsonObject { ["const"] = 0 },
                new JsonObject { ["minimum"] = 320, ["maximum"] = 8192 }
            },
            ["default"] = defaultMaxWidth,
            ["description"] = "Maximum encoded width. Use 0 for native width; nonzero values must be at least 320."
        },
        ["image_format"] = EnumProperty(["jpeg", "png"], "jpeg"),
        ["jpeg_quality"] = NumberProperty(0.82, 0.1, 1)
    };

    private static JsonObject Merge(JsonObject first, JsonObject second)
    {
        var merged = new JsonObject();
        foreach (var pair in first)
            merged[pair.Key] = pair.Value?.DeepClone();
        foreach (var pair in second)
            if (!merged.ContainsKey(pair.Key))
                merged[pair.Key] = pair.Value?.DeepClone();
        return merged;
    }

    private static JsonObject NumberProperty(double? defaultValue = null, double? minimum = null, double? maximum = null, string? description = null)
    {
        var property = new JsonObject { ["type"] = "number" };
        if (defaultValue is not null) property["default"] = defaultValue;
        if (minimum is not null) property["minimum"] = minimum;
        if (maximum is not null) property["maximum"] = maximum;
        if (description is not null) property["description"] = description;
        return property;
    }

    private static JsonObject IntegerProperty(int? defaultValue = null, int? minimum = null, int? maximum = null)
    {
        var property = new JsonObject { ["type"] = "integer" };
        if (defaultValue is not null) property["default"] = defaultValue;
        if (minimum is not null) property["minimum"] = minimum;
        if (maximum is not null) property["maximum"] = maximum;
        return property;
    }

    private static JsonObject EnumProperty(string[] values, string defaultValue) => new()
    {
        ["type"] = "string",
        ["enum"] = new JsonArray(values.Select(value => JsonValue.Create(value)).ToArray()),
        ["default"] = defaultValue
    };

    private static JsonObject CaptureAfterProperty() => new()
    {
        ["type"] = "boolean",
        ["default"] = true,
        ["description"] = "Return the post-action screenshot. Set false only for a deterministic intermediate keyboard step; the next visible state change must still be observed."
    };

    private static JsonObject SettleProperty(int defaultValue) => new()
    {
        ["type"] = "integer", ["minimum"] = 0, ["maximum"] = 5000,
        ["default"] = defaultValue,
        ["description"] = "Delay after input before capturing the result."
    };
}
