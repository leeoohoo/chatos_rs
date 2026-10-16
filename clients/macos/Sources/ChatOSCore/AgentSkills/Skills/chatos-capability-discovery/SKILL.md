---
name: chatos-capability-discovery
description: Discover and invoke only the run-approved ChatOS built-in or installed Plugin capability needed for the current Todo, while progressively activating its required product or Plugin Skills.
---

# Capability discovery

When the task needs project files, terminal operations, requirement surveys, or an installed Plugin:

1. Call `capability_search` with short task-specific keywords. Do not enumerate capabilities merely to explore.
2. Call `capability_describe` for the best matching current-run `plugin_option`. For an index, omit `tool_option` and `schema_offset`, or set them to null; never fill them with placeholders or 0. This returns a short Skill catalog and a paged tool index, not callable schemas. Activate the relevant router when needed to choose the workflow. Use `query` with tool names or operation keywords to narrow the index, or `offset=next_offset` to continue it; do not enumerate unrelated tools.
3. Call `capability_describe` again with the selected exact `plugin_option` and `tool_option` to read only that tool's complete schema and gate; omit `query` and `schema_offset` or set them to null. Only if `schema_delivery=paged`, explicitly read `schema_offset=next_schema_offset`, concatenate chunks in order with the same hash until the next offset is null, and never guess omitted fields. If an old Run lacks the advertised parameters, report that execution must restart with current capabilities; do not fall back to the old all-tools disclosure or invent unsupported arguments. Activate only the listed `skill_ref` values needed by the chosen tool. Pass each exact ref to `agent_skill_activate`; never reconstruct a ref from a display name. An undisclosed Plugin Skill ref is rejected even if its name happens to exist in the selected Release. The activation result already contains that Skill's resource paths. Read only a reference needed for the current decision with `agent_skill_read_resource`; after resume, reactivate the same exact ref if the earlier result is no longer in context.
4. Follow the described `invocation_mode`: for `direct_task_tool`, call the named native Task tool directly after activation; otherwise call `capability_invoke` with the current-run `plugin_option` and `tool_option`. Search again only when the task genuinely needs a different capability family.

Options, project identity, filesystem root, Plugin Release, and local permissions are bound by ChatOS. Never guess, request, persist, or echo their internal identifiers. Skill activation explains an already authorized capability; it does not expand Todo, project, Plugin, or Human approval permissions.

Read [references/search-activation-and-recovery.md](references/search-activation-and-recovery.md) for selection, dynamic Plugin gates, stale options, missing Skills, and invocation recovery.
