# Capability search, activation, and recovery

## Search and describe

Good: search with the task outcome and one distinguishing operation, then describe the single best match.

Bad: use an empty or generic query, enumerate every installed Plugin, or treat a display name as proof that a tool is suitable.

Every `plugin_option` and `tool_option` is scoped to the current run. Never reuse an option from memory, another run, or an earlier description after the runtime reports it stale.

## Progressive Skill activation

`capability_describe` returns `required_skills` per tool and a short catalog. Product Skills come from the ChatOS bundle; Plugin Skills come from the selected installed Release's frozen snapshot. Activate only those required by the intended call.

A Plugin gate can depend on an argument value. Describe may list every possible leaf, but the invoke gate requires only the router and the leaf selected by the actual arguments. Do not activate all alternatives preemptively.

## Invoke and recover

- Missing Skill: activate the named current-capability Skill, then retry once with the same business arguments.
- Missing selector or unmapped selector value: correct the tool arguments from its schema; do not bypass the gate.
- Stale option: search and describe again, then use the newly returned option.
- Permission or Human approval refusal: stop or report the refusal; Skill activation cannot override it.
- Plugin startup or fixed-snapshot failure: report that the installed capability is unavailable or inconsistent. Do not fall back to an unverified executable.
- Tool error after execution: inspect the returned result and the specialist Skill's recovery guidance before deciding whether a retry is safe.
