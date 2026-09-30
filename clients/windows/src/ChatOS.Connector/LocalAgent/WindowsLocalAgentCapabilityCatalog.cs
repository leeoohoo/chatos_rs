using System.Text.Json;

namespace ChatOS.Connector.LocalAgent;

internal static class WindowsLocalAgentCapabilityCatalog
{
    public const string Revision = "native-windows-main-chat-v3";

    public static IReadOnlyList<JsonElement> MainChatTools { get; } =
    [
        Parse("""
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
        """),
        Parse("""
        {
          "type": "function",
          "name": "create_task",
          "description": "Create one durable local task derived from the current conversation. Use it only for user-requested tracked work. The Rust Local Agent Host persists and schedules it locally.",
          "parameters": {
            "type": "object",
            "properties": {
              "title": { "type": "string", "minLength": 1 },
              "objective": { "type": "string", "minLength": 1 },
              "description": { "type": "string" },
              "input_payload": { "type": "object" }
            },
            "required": ["title", "objective"],
            "additionalProperties": false
          }
        }
        """),
        Parse("""
        {
          "type": "function",
          "name": "create_tasks_with_prerequisites",
          "description": "Create a durable local task graph. Each task uses a unique client_ref and prerequisite_refs may only reference tasks in this call. The Rust Local Agent Host persists and schedules the DAG locally.",
          "parameters": {
            "type": "object",
            "properties": {
              "tasks": {
                "type": "array",
                "minItems": 1,
                "maxItems": 50,
                "items": {
                  "type": "object",
                  "properties": {
                    "client_ref": { "type": "string", "minLength": 1 },
                    "title": { "type": "string", "minLength": 1 },
                    "objective": { "type": "string", "minLength": 1 },
                    "description": { "type": "string" },
                    "input_payload": { "type": "object" },
                    "prerequisite_refs": {
                      "type": "array",
                      "items": { "type": "string", "minLength": 1 },
                      "uniqueItems": true
                    }
                  },
                  "required": ["client_ref", "title", "objective"],
                  "additionalProperties": false
                }
              }
            },
            "required": ["tasks"],
            "additionalProperties": false
          }
        }
        """),
    ];

    public static IReadOnlyList<JsonElement> TaskExecutionTools { get; } =
    [
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

    public static IReadOnlySet<string> ProjectToolNames { get; } = new HashSet<string>(
        ["project_list", "project_read", "project_search", "project_write", "terminal_exec"],
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
