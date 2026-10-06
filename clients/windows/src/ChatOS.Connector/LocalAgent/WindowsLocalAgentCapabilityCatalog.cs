using System.Text.Json;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalAgentPluginChoice(
    string PluginKey,
    string DisplayName,
    string Description);

internal sealed record WindowsLocalAgentMcpChoice(string Value, string Title);

internal static class WindowsLocalAgentCapabilityCatalog
{
    public const string Revision = "native-windows-main-chat-v3";

    public static IReadOnlyList<JsonElement> MainChatTools { get; } = MainChatToolsFor([]);

    public static IReadOnlyList<JsonElement> MainChatToolsFor(
        IReadOnlyList<WindowsLocalAgentPluginChoice> pluginChoices,
        IReadOnlyList<WindowsLocalAgentMcpChoice>? builtinChoices = null,
        IReadOnlyList<WindowsLocalAgentMcpChoice>? externalChoices = null)
    {
        builtinChoices ??= [];
        externalChoices ??= [];
        var pluginKeys = pluginChoices
            .OrderBy(choice => choice.PluginKey, StringComparer.Ordinal)
            .Select(choice => choice.PluginKey)
            .ToArray();
        var pluginTitles = pluginChoices
            .OrderBy(choice => choice.PluginKey, StringComparer.Ordinal)
            .Select(choice => $"{choice.DisplayName} — {choice.Description}")
            .ToArray();
        var maxItems = pluginKeys.Length == 0 ? 0 : 16;
        return
        [
            Parse(JsonSerializer.Serialize(new
            {
                type = "function",
                name = "create_task",
                parameters = new
                {
                    type = "object",
                    properties = new
                    {
                        enabled_builtin_kinds = McpSelection(builtinChoices),
                        external_mcp_config_ids = McpSelection(externalChoices),
                        plugin_hints = PluginHints(pluginKeys, pluginTitles, maxItems),
                    },
                },
            })),
            Parse(JsonSerializer.Serialize(new
            {
                type = "function",
                name = "create_tasks_with_prerequisites",
                parameters = new
                {
                    type = "object",
                    properties = new
                    {
                        tasks = new
                        {
                            type = "array",
                            items = new
                            {
                                type = "object",
                                properties = new
                                {
                                    enabled_builtin_kinds = McpSelection(builtinChoices),
                                    external_mcp_config_ids = McpSelection(externalChoices),
                                    plugin_hints = PluginHints(pluginKeys, pluginTitles, maxItems),
                                },
                            },
                        },
                    },
                },
            })),
        ];
    }

    private static object McpSelection(IReadOnlyList<WindowsLocalAgentMcpChoice> choices)
    {
        var ordered = choices
            .DistinctBy(choice => choice.Value, StringComparer.Ordinal)
            .OrderBy(choice => choice.Value, StringComparer.Ordinal)
            .ToArray();
        return new
        {
            type = "array",
            maxItems = ordered.Length == 0 ? 0 : (int?)null,
            uniqueItems = true,
            items = new
            {
                type = "string",
                minLength = 1,
                @enum = ordered.Select(choice => choice.Value).ToArray(),
                oneOf = ordered.Select(choice => new
                {
                    @const = choice.Value,
                    title = choice.Title,
                }).ToArray(),
            },
        };
    }

    private static object PluginHints(
        IReadOnlyList<string> pluginKeys,
        IReadOnlyList<string> pluginTitles,
        int maxItems) => new
    {
        type = "array",
        maxItems,
        uniqueItems = true,
        description = pluginKeys.Count == 0
            ? "No installed Plugin is selectable for this request. Send an empty plugin_hints array."
            : "Suggest only the minimum installed Plugins required by this Task.",
        items = new
        {
            type = "object",
            properties = new
            {
                plugin_key = new
                {
                    type = "string",
                    minLength = 1,
                    @enum = pluginKeys,
                    oneOf = pluginKeys.Zip(pluginTitles, (key, title) => new
                    {
                        @const = key,
                        title,
                    }).ToArray(),
                },
                reason = new { type = "string", maxLength = 1000 },
            },
            required = new[] { "plugin_key" },
            additionalProperties = false,
        },
    };

    private static JsonElement AttachmentReadTool { get; } = Parse("""
        {
          "type": "function",
          "name": "local_attachment_read",
          "description": "Read a bounded segment of an authorized local attachment without exposing its filesystem path.",
          "parameters": {
            "type": "object",
            "properties": {
              "authorized_local_ref": { "type": "string", "minLength": 1, "maxLength": 160 },
              "offset": { "type": "integer", "minimum": 0, "default": 0 },
              "limit": { "type": "integer", "minimum": 1, "maximum": 65536, "default": 16384 }
            },
            "required": ["authorized_local_ref"],
            "additionalProperties": false
          }
        }
        """);

    private static IReadOnlyList<JsonElement> BaseTaskExecutionTools { get; } =
    [
        AttachmentReadTool,
        Parse("""
        {
          "type": "function",
          "name": "project_list",
          "description": "List directories and files inside the project bound to this local task.",
          "parameters": {
            "type": "object",
            "properties": {
              "path": { "type": "string", "description": "Project-relative directory; defaults to ." },
              "include_files": { "type": "boolean", "default": true }
            },
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "project_read",
          "description": "Read a text or small binary file inside the project bound to this local task.",
          "parameters": {
            "type": "object",
            "properties": {
              "path": { "type": "string", "minLength": 1 }
            },
            "required": ["path"],
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "project_search",
          "description": "Search project-relative file names or text content.",
          "parameters": {
            "type": "object",
            "properties": {
              "query": { "type": "string", "minLength": 1 },
              "path": { "type": "string", "description": "Project-relative search root; defaults to ." },
              "mode": { "type": "string", "enum": ["name", "content"], "default": "content" },
              "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 50 }
            },
            "required": ["query"],
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "project_write",
          "description": "Atomically create or update one UTF-8 text file inside the project. Host approval is required before execution.",
          "parameters": {
            "type": "object",
            "properties": {
              "path": { "type": "string", "minLength": 1 },
              "content": { "type": "string" },
              "create_only": { "type": "boolean", "default": false }
            },
            "required": ["path", "content"],
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "terminal_exec",
          "description": "Execute a bounded non-interactive command inside the project. Host approval is required before execution.",
          "parameters": {
            "type": "object",
            "properties": {
              "command": { "type": "string", "minLength": 1 },
              "arguments": {
                "type": "array",
                "items": { "type": "string" },
                "maxItems": 100
              },
              "working_directory": { "type": "string", "description": "Project-relative directory; defaults to ." },
              "timeout_ms": { "type": "integer", "minimum": 1000, "maximum": 900000 }
            },
            "required": ["command"],
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "capability_search",
          "description": "Search enabled local Plugins by task keywords without starting a Plugin runtime.",
          "parameters": {
            "type": "object",
            "properties": {
              "query": { "type": "string", "minLength": 1, "maxLength": 200 }
            },
            "required": ["query"],
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "capability_describe",
          "description": "Lazily start one Plugin returned by capability_search and describe its run-scoped local tools.",
          "parameters": {
            "type": "object",
            "properties": {
              "plugin_option": { "type": "string", "minLength": 1, "maxLength": 80 }
            },
            "required": ["plugin_option"],
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "capability_skill_activate",
          "description": "Activate one fixed Plugin Skill required by tools returned from capability_describe and return its immutable instructions and resource index.",
          "parameters": {
            "type": "object",
            "properties": {
              "plugin_option": { "type": "string", "minLength": 1, "maxLength": 80 },
              "skill_name": { "type": "string", "minLength": 1, "maxLength": 120 }
            },
            "required": ["plugin_option", "skill_name"],
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "capability_skill_read_resource",
          "description": "Read a bounded page of a text resource belonging to an activated fixed Plugin Skill.",
          "parameters": {
            "type": "object",
            "properties": {
              "plugin_option": { "type": "string", "minLength": 1, "maxLength": 80 },
              "skill_name": { "type": "string", "minLength": 1, "maxLength": 120 },
              "relative_path": { "type": "string", "minLength": 1, "maxLength": 500 },
              "offset": { "type": "integer", "minimum": 0 },
              "limit": { "type": "integer", "minimum": 1, "maximum": 64000 }
            },
            "required": ["plugin_option", "skill_name", "relative_path"],
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "capability_invoke",
          "description": "Invoke a local Plugin tool previously returned by capability_describe. Plugin permissions and per-call approval remain enforced by the native client.",
          "parameters": {
            "type": "object",
            "properties": {
              "plugin_option": { "type": "string", "minLength": 1, "maxLength": 80 },
              "tool_option": { "type": "string", "minLength": 1, "maxLength": 80 },
              "arguments": { "type": "object" }
            },
            "required": ["plugin_option", "tool_option", "arguments"],
            "additionalProperties": false
          }
        }
        """),
    ];

    public const string RemoteConnectionToolPrefix = "remote_connection_controller_";

    private static IReadOnlyList<JsonElement> RemoteConnectionTools { get; } =
    [
        Parse("""
        {
          "type": "function",
          "name": "remote_connection_controller_test_connection",
          "description": "Test the remote connection selected by the user for this conversation.",
          "parameters": { "type": "object", "properties": {}, "additionalProperties": false }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "remote_connection_controller_run_command",
          "description": "Run one SSH command on the remote connection selected by the user. Host approval is required.",
          "parameters": {
            "type": "object",
            "properties": {
              "command": { "type": "string", "minLength": 1 },
              "working_directory": { "type": "string" },
              "allow_dangerous": { "type": "boolean" },
              "max_output_chars": { "type": "integer", "minimum": 1, "maximum": 20000 }
            },
            "required": ["command"],
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "remote_connection_controller_list_directory",
          "description": "List entries under a directory on the remote connection selected by the user.",
          "parameters": {
            "type": "object",
            "properties": {
              "path": { "type": "string" },
              "limit": { "type": "integer", "minimum": 1, "maximum": 1000 }
            },
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "remote_connection_controller_read_file",
          "description": "Read a bounded UTF-8 text file from the remote connection selected by the user.",
          "parameters": {
            "type": "object",
            "properties": {
              "path": { "type": "string", "minLength": 1 },
              "max_bytes": { "type": "integer", "minimum": 1, "maximum": 262144 }
            },
            "required": ["path"],
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "remote_connection_controller_download_file",
          "description": "Download bounded file content from the remote connection selected by the user.",
          "parameters": {
            "type": "object",
            "properties": {
              "path": { "type": "string", "minLength": 1 },
              "encoding": { "type": "string", "enum": ["text", "base64"] },
              "max_bytes": { "type": "integer", "minimum": 1, "maximum": 262144 }
            },
            "required": ["path"],
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "remote_connection_controller_upload_file",
          "description": "Upload bounded content to the remote connection selected by the user. Host approval is required.",
          "parameters": {
            "type": "object",
            "properties": {
              "path": { "type": "string", "minLength": 1 },
              "content": { "type": "string" },
              "encoding": { "type": "string", "enum": ["text", "base64"] },
              "create_parent_dirs": { "type": "boolean" },
              "overwrite": { "type": "boolean" }
            },
            "required": ["path", "content"],
            "additionalProperties": false
          }
        }
        """),
    ];

    public static IReadOnlyList<JsonElement> TaskExecutionTools { get; } =
        BaseTaskExecutionTools.Concat(RemoteConnectionTools).ToArray();

    public static IReadOnlyList<JsonElement> TaskExecutionToolsFor(
        IReadOnlyList<WindowsLocalAgentExternalMcpTool> externalTools) =>
        TaskExecutionTools.Concat(externalTools.Select(value => value.ModelTool())).ToArray();

    public static IReadOnlySet<string> ProjectToolNames { get; } = new HashSet<string>(
        [
            "project_list", "project_read", "project_search", "project_write", "terminal_exec",
            "remote_connection_controller_test_connection",
            "remote_connection_controller_run_command",
            "remote_connection_controller_list_directory",
            "remote_connection_controller_read_file",
            "remote_connection_controller_download_file",
            "remote_connection_controller_upload_file",
        ],
        StringComparer.Ordinal);

    public static IReadOnlySet<string> PluginToolNames { get; } = new HashSet<string>(
        [
            "capability_search", "capability_describe", "capability_skill_activate",
            "capability_skill_read_resource", "capability_invoke",
        ],
        StringComparer.Ordinal);

    public static IReadOnlySet<string> TaskExecutionToolNames { get; } =
        TaskExecutionTools
            .Select(tool => tool.GetProperty("name").GetString()!)
            .ToHashSet(StringComparer.Ordinal);

    private static JsonElement Parse(string json)
    {
        using var document = JsonDocument.Parse(json);
        return document.RootElement.Clone();
    }
}
