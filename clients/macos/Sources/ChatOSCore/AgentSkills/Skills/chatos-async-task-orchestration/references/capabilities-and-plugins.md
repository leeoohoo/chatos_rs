# Capabilities and plugins

Select capabilities per Task, not once for the whole graph.

- `CodeMaintainerRead`: inspect and search project files.
- `CodeMaintainerWrite`: create, modify, or delete project files; read access is added by policy.
- `TerminalController`: commands, dependency tools, Git, tests, type checks, builds, or runtime verification; read access is added by policy.
- Requirement Survey capabilities: inspect or create structured requirement surveys when present in the schema.
- `Notepad`: read or write durable notes only when the user explicitly asks to record/save in Notepad, or identifies an existing note as Task input. For an optional selection, copy the exact authorizing phrase into `notepad_user_request_quote`; never paraphrase or cite generic task text. Never select it for scratch work, internal plans, generic persistence, cross-Task convenience, or as a fallback for a Plugin deliverable.

Choose only values present in the current schema. Set `requires_execution=true` for commands or file mutation. Pure project analysis can remain read-only.

Use plugin hints only from the current catalog. Each hint must name the concrete operation or output requiring it. Bind a plugin only to the Tasks that use it. Never invent generic plugin keys. The runtime freezes the selected Plugin Release and artifact hash into the Task; if that Release changes before execution, create replacement work against the newly advertised capability instead of silently substituting it. If a required capability is absent, use an in-scope alternative or report the exact limitation instead of looping.

When the user explicitly names an installed Plugin or MCP in natural language and asks that it perform an operation, include its exact catalog component in `plugin_hints` for every Task that performs that operation. Treat supporting capabilities as additive, never as substitutes: a Task may select both the named reverse-engineering Plugin and a browser controller when each owns distinct evidence or actions. Do not drop the named component merely because automatic routing finds another capability suitable for the target type. If the named component is unavailable in the request-scoped catalog, report that limitation rather than claiming the requested route was used.

An explicit Plugin chosen in the Main Chat composer is program-owned user intent, not a suggestion. Preserve it on the created Task exactly; do not add unrelated optional Plugins or reroute the turn into Requirement Survey. The survey picker is a separate explicit user choice; missing Plugin task details belong to Ask User within the created Task. If the frozen catalog no longer contains that selection, report the concrete availability error rather than asking the user to supply a version or internal identifier.

Treat structured requirements, solution design, and dependency-aware implementation plans as planning artifacts. When the catalog advertises a matching solution-planning Plugin, select it for the Task even if the same information could be flattened into ordinary prose or a Markdown file. Use `CodeMaintainerWrite` for a project Markdown plan only when the user explicitly asked for that file form or the matching planning Plugin is unavailable.

If the user explicitly requests both a Plugin artifact and a Notepad record, keep the delivery surfaces separate: the Plugin Task creates and verifies the managed artifact, then a downstream Notepad Task records only the requested note. Do not give writable Notepad to the Plugin Task.

Plugin and External MCP effects are not currently confined to an enforceable project-relative resource range. A Task selecting either is therefore a whole-project writer for runtime scheduling, even when the intended operation appears read-only. Independent read-only Tasks may still proceed, but another potential writer waits. Add a prerequisite when two such Tasks require a deterministic order; do not rely on scheduler order.

Select `default_model_config_id` per Task from the current eligible model catalog, using its usage descriptions to match the work's complexity and honoring the user's model preference. Choose `thinking_level` for that Task's reasoning needs rather than routinely omitting it; `auto` intentionally uses the model default. Never fabricate model IDs or unsupported reasoning levels. If no eligible catalog is advertised, omit the model ID and use the runtime fallback. Do not place credentials or sensitive raw data in titles, objectives, tags, rationale, or hints; the platform owns account, conversation, workspace, and callback scope.
