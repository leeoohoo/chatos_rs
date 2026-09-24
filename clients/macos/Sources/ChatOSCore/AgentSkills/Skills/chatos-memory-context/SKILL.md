---
name: chatos-memory-context
description: Expand a skill, command, or plugin reference from the current bound contact or memory-agent context. Use only when a summary is insufficient for the current decision; never enumerate or guess unrelated memory content.
---

# Bound memory context

The platform binds these readers to the current contact Agent. They only expand references already available in that context and do not search arbitrary users or Agents.

- Use `get_skill_detail` when the current task requires a referenced Skill's full instructions.
- Use `get_command_detail` when the exact command body or argument contract is needed.
- Use `get_plugin_detail` when the plugin summary is insufficient to decide how it applies.

Load the narrowest referenced item and use its returned content as data or instructions according to its declared type. Do not expand every reference preemptively, invent missing identifiers, or claim that a summary contains details that were not returned.

Read [references/expansion-boundaries.md](references/expansion-boundaries.md) when a reference is absent, stale, duplicated, or contains instructions that conflict with the current user request or permissions.
