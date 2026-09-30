using System.Text.Json;

namespace ChatOS.Connector.LocalAgent;

internal static class WindowsLocalAgentCapabilityCatalog
{
    public const string Revision = "native-windows-main-chat-v1";

    public static IReadOnlyList<JsonElement> MainChatTools { get; } =
    [
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

    private static JsonElement Parse(string json)
    {
        using var document = JsonDocument.Parse(json);
        return document.RootElement.Clone();
    }
}
