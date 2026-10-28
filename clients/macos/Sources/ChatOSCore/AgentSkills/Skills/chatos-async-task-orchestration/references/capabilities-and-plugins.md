# Capabilities and plugins

Select capabilities per Task, not once for the whole graph.

For the common uncertain-defect graph, keep authority phase-specific:

- `diagnose`: read access plus only the observation capabilities required to reproduce and locate the problem; no `CodeMaintainerWrite`.
- `fix`: `CodeMaintainerWrite` and, when focused commands are required, `TerminalController`; do not carry an observation Plugin merely because another phase needs it.
- `verify`: the observation and command capabilities required to independently exercise the repaired behavior; no `CodeMaintainerWrite`.

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

Do not place planning persistence on the implementation critical path merely because the planning Plugin is selected. If the user requested both a managed planning artifact and implementation, separate the artifact writeback from the executable work unless the implementation truly requires an unresolved decision inside that artifact. Link auxiliary persistence, status updates, exports, or summaries with `context_refs`; reserve `prerequisite_refs` for the narrow decision or evidence that must exist before implementation can begin. A Plugin persistence error must not stop otherwise authorized code or tests.

Keep Plugin contracts outcome-based. Do not freeze a Plugin's workspace ID, active record, revision lineage, or exact mutation sequence unless the user explicitly made that identity part of the requirement. In particular, a request to remove the current plan and rebuild it from scratch must permit a planning Plugin's documented archive-and-restart or equivalent history-preserving reset; do not reinterpret creation of the replacement active record as an unrelated parallel deliverable or “escaping” cleanup. For ordinary revisions, preserve the active artifact and do not reset it.

If the user explicitly requests both a Plugin artifact and a Notepad record, keep the delivery surfaces separate: the Plugin Task creates and verifies the managed artifact, then a downstream Notepad Task records only the requested note. Do not give writable Notepad to the Plugin Task.

Plugin and External MCP effects are not currently confined to an enforceable project-relative resource range. A Task selecting either is therefore a whole-project writer for runtime scheduling, even when the intended operation appears read-only. Independent read-only Tasks may still proceed, but another potential writer waits. Add a prerequisite when two such Tasks require a deterministic order; do not rely on scheduler order.

For browser observation or verification, request the smallest evidence that proves the criterion. URL plus accessibility state is the default for ordinary interaction checks. Request full DOM, response bodies, or per-step captures only for the specific step whose claim requires them. Do not mechanically capture full DOM after every action; repeated page snapshots create large duplicate results and extra model reads without strengthening the evidence.

Select `default_model_config_id` per Task from the current eligible model catalog, using its usage descriptions to match the work's complexity and honoring the user's model preference. Choose `thinking_level` from the reasoning actually required by that Task:

- Use `medium` for implementation from an established diagnosis or checklist, routine browser acceptance, and other bounded work whose success criteria are already explicit.
- Use `high` only when the Task must resolve substantial ambiguity, compare multiple plausible architectures or causes, or make a consequential synthesis that cannot be reduced to a known checklist.
- Do not select `high` merely because a Task has many mechanical steps, uses a Plugin, or performs broad regression coverage. Split or batch repetitive evidence collection instead of increasing reasoning effort.
- Use `auto` only when intentionally delegating the choice to the model configuration; omission uses the runtime fallback and must not be treated as an implicit request for `high`.

Never fabricate model IDs or unsupported reasoning levels. If no eligible catalog is advertised, omit the model ID and use the runtime fallback. Do not place credentials or sensitive raw data in titles, objectives, tags, rationale, or hints; the platform owns account, conversation, workspace, and callback scope.
