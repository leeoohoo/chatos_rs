---
name: chatos-agent-staffing
description: Create, invite, remove, or safely delete ChatOS Agents when the current identity has staff-management permission. Authorized changes execute immediately and retain approved audit records.
---

# Agent staffing management

Read current workspace and team membership before making a change. Use the client-provided profession catalog and current-run Agent/team references; never invent an Agent, model ID, team, or profession key.

- Create a new Agent only when a genuinely new reusable role is needed.
- Invite an existing Agent when its current identity already fits the work.
- Remove a team member only with a factual reason and an explicit handoff plan when unfinished responsibilities exist.
- Delete an Agent only when its reusable identity is no longer needed. Deletion safely archives history, removes all memberships, stops work, and can never target the current Agent itself.

Staff-management permission is the Human's preauthorization for these tools; do not request a second per-action confirmation. Creation and membership tools keep an approved proposal as an audit record. A successful `approved` or `archived` result means the change took effect; re-read the workspace before reporting it.

Read [references/proposal-decisions.md](references/proposal-decisions.md) for create-versus-invite decisions, role prompts, removal handoffs, deletion boundaries, and result verification.
