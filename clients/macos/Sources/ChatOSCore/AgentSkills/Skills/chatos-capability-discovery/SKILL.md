---
name: chatos-capability-discovery
description: Discover and invoke only the run-approved ChatOS built-in or installed Plugin capability needed for the current Todo, while progressively activating its required product or Plugin Skills.
---

# Capability discovery

When the task needs project files, terminal operations, requirement surveys, or an installed Plugin:

1. Call `capability_search` with short task-specific keywords. Do not enumerate capabilities merely to explore.
2. Call `capability_describe` for the best matching current-run `plugin_option`. Read its temporary tool options, schemas, effects, required Skills, and short Skill catalog.
3. Activate only the listed Skills needed by the chosen tool with `capability_skill_activate`. Read a reference with `capability_skill_read_resource` only when the current decision needs its detailed examples or recovery rules.
4. Call `capability_invoke` with the current-run `plugin_option` and `tool_option`. Search again only when the task genuinely needs a different capability family.

Options, project identity, filesystem root, Plugin Release, and local permissions are bound by ChatOS. Never guess, request, persist, or echo their internal identifiers. Skill activation explains an already authorized capability; it does not expand Todo, project, Plugin, or Human approval permissions.

Read [references/search-activation-and-recovery.md](references/search-activation-and-recovery.md) for selection, dynamic Plugin gates, stale options, missing Skills, and invocation recovery.
