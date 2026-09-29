---
name: chatos-relay-context
description: Read the current Agent's global unread messages, attachments, and account-level workspace references without treating one room or project as the runtime scope.
---

# Agent communication context

Begin from Agent-level facts:

- `chat_read_all_unread` reads unread messages across every group and direct conversation for the current Agent and advances the returned cursors.
- `agent_workspace_snapshot` supplies current-run team, Agent, and explicit project-manager references for proactive routing and task planning.
- Use `chat_read_attachment` only with message and attachment references returned in this Run.

Temporary references are scoped capabilities, not stable IDs. Never invent, persist externally, or substitute a reference from another Run. Reading one conversation does not authorize sending to another.

Read [references/references-and-unread.md](references/references-and-unread.md) for unread handling, attachment limits, and cross-team routing preparation.
