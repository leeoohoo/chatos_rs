---
name: chatos-skill-runtime
description: Operate the run-scoped ChatOS Skill catalog by activating listed identity or product Skills and reading only the references needed for the current decision. This control plane never grants tools or permissions.
---

# ChatOS Skill runtime

Use only `skill_ref` values returned in the current Run's Router or product Skill catalog.

- Call `agent_skill_activate` before relying on an on-demand Skill's detailed rules.
- List or read resources only after that Skill is activated.
- Read the smallest relevant page and continue from `next_offset` only when more content is needed.
- Never construct a Skill reference, switch to an unlisted identity, or treat activation as authorization for a tool, project, Plugin, or side effect.

System-required Skills may already be active. Product tools can still reject a call when a separate permission, Todo capability, revision, or Human confirmation is missing.
