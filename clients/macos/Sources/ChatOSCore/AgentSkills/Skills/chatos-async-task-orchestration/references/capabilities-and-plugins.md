# Capabilities and plugins

Select capabilities per Task, not once for the whole graph.

- `CodeMaintainerRead`: inspect and search project files.
- `CodeMaintainerWrite`: create, modify, or delete project files; read access is added by policy.
- `TerminalController`: commands, dependency tools, Git, tests, type checks, builds, or runtime verification; read access is added by policy.
- Requirement Survey capabilities: inspect or create structured requirement surveys when present in the schema.
- `Notepad`: durable Task notes when needed.

Choose only values present in the current schema. Set `requires_execution=true` for commands or file mutation. Pure project analysis can remain read-only.

Use plugin hints only from the current catalog. Each hint must name the concrete operation or output requiring it. Bind a plugin only to the Tasks that use it. Never invent generic plugin keys. The runtime freezes the selected Plugin Release and artifact hash into the Task; if that Release changes before execution, create replacement work against the newly advertised capability instead of silently substituting it. If a required capability is absent, use an in-scope alternative or report the exact limitation instead of looping.

Plugin and External MCP effects are not currently confined to an enforceable project-relative resource range. A Task selecting either is therefore a whole-project writer for runtime scheduling, even when the intended operation appears read-only. Independent read-only Tasks may still proceed, but another potential writer waits. Add a prerequisite when two such Tasks require a deterministic order; do not rely on scheduler order.

Select `default_model_config_id` per Task from the current eligible model catalog, using its usage descriptions to match the work's complexity and honoring the user's model preference. Choose `thinking_level` for that Task's reasoning needs rather than routinely omitting it; `auto` intentionally uses the model default. Never fabricate model IDs or unsupported reasoning levels. If no eligible catalog is advertised, omit the model ID and use the runtime fallback. Do not place credentials or sensitive raw data in titles, objectives, tags, rationale, or hints; the platform owns account, conversation, workspace, and callback scope.
