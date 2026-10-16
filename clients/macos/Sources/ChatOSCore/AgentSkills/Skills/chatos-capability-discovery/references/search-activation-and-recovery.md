# Capability search, activation, and recovery

## Search and describe

Good: search with the task outcome and one distinguishing operation, then describe the single best match.

Bad: use an empty or generic query, enumerate every installed Plugin, or treat a display name as proof that a tool is suitable.

Every `plugin_option` and `tool_option` is scoped to the current run. Never reuse an option from memory, another run, or an earlier description after the runtime reports it stale.

For `disclosure_version=2`, the default `view=catalog` has no input schemas. Narrow it with `query` (tool names/keywords), or read the next index page at `offset=next_offset`. Then describe the chosen `tool_option` to get `view=tool`, its complete gate and schema. Tool options stay stable while filtering/paging within that Run; an index is not schema evidence or invocation authorization. Reading tool B or another catalog page does not revoke a previously described tool A.

The three request modes are distinct. Unused optional fields may be omitted or explicitly null; never invent placeholder option strings, and never use 0 as an unset `schema_offset`:

- Index: `{"plugin_option":"<search result>","query":"browser_session_open navigate snapshot","tool_option":null,"schema_offset":null}`. `offset` and `limit` default to 0 and 8 if omitted or null.
- Selected tool: `{"plugin_option":"<search result>","tool_option":"<exact index option>","query":null,"schema_offset":null}`.
- Explicit schema page: same selected tool and null query, plus the integer `schema_offset` returned in `next_schema_offset`. Request this only after a response declares `schema_delivery=paged`.

If a describe call reports invalid arguments, correct the mode using these rules and the concrete error; do not repeat the same incompatible combination or change a fake option to another fake option.

Oversized schemas use explicit pages: keep the same tool and `schema_sha256`, read `schema_offset=next_schema_offset` until null, and concatenate `schema_chunk` by Unicode scalar offsets. Every response is valid JSON; chunks are not individually complete schemas. `capability_description_budget_exceeded` or `capability_description_entry_too_large` is a concrete transport/metadata limitation, not proof that the browser or service is down. Do not use a surviving unrelated tool or raw CDP as a guessed replacement for a missing definition. Runs without the current parameters must restart with current capabilities; there is no all-tools legacy fallback.

## Progressive Skill activation

`capability_describe` returns unified `required_skill_refs` per tool and a short catalog. Product refs use `product-skill:<name>`; Plugin refs use the opaque run-scoped form `plugin-skill:<plugin_option>:<name>`. Product Skills come from the ChatOS bundle; Plugin Skills come from the selected installed Release's frozen snapshot. Activate only the exact refs required by the intended call through `agent_skill_activate`. A Plugin ref must first have appeared in a successful current-Run describe response; knowing or guessing a Skill name is not disclosure. Its result includes the available resource paths, so read only the relevant one with `agent_skill_read_resource`; if a resumed context no longer contains the activation result, activate the same exact ref again. Skill control accepts the exact `skill_ref`; do not pass a Plugin option and Skill name as a substitute.

A Plugin gate can depend on an argument value. Describe may list every possible leaf, but the invoke gate requires only the router and the leaf selected by the actual arguments. Do not activate all alternatives preemptively.

The canonical `skill_gate` declares `all_of` refs and optional `select_by_argument` with a JSON-pointer `pointer` and value-to-ref `map`. Evaluate it against the intended business arguments, not the broker wrapper. A missing, non-string, or unmapped selector is an invalid call, not a reason to activate every leaf. A successful describe is durable for the current Run; fabricated options or descriptions from another Run do not satisfy the execution gate.

## Invoke and recover

Task built-ins report `invocation_mode=direct_task_tool`. Activate their required Product Skills, then call the described native tool directly. This preserves durable Task approval, project binding, edit-session ownership, and terminal process ownership. A Task broker rejects built-in `capability_invoke` even when its Skills have already been activated. Plugin tools and group-chat built-ins report `invocation_mode=capability_invoke`.

External HTTP MCP tools are already exposed as direct `external_mcp__…` names. If the Task catalog lists `product-skill:chatos-external-mcp`, activate it and load the relevant references before calling those tools. Do not manufacture Plugin options for them. Their product guidance is not a substitute for unavailable server-owned business Skills.

- Missing Skill: activate the returned current-run `skill_ref`, then retry once with the same business arguments.
- Missing selector or unmapped selector value: correct the tool arguments from its schema; do not bypass the gate.
- Stale option: search and describe again, then use the newly returned option.
- Permission or Human approval refusal: stop or report the refusal; Skill activation cannot override it.
- Plugin startup or fixed-snapshot failure: report that the installed capability is unavailable or inconsistent. Do not fall back to an unverified executable.
- Tool error after execution: inspect the returned result and the specialist Skill's recovery guidance before deciding whether a retry is safe.
