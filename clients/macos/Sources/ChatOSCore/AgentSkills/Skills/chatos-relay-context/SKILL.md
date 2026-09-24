---
name: chatos-relay-context
description: Read the identity-bound ChatOS Relay context, triggers, members, messages, attachments, unread state, and workspace references without crossing the current run's account, project, room, or agent scope.
---

# Relay context

Begin from the smallest authoritative context source:

- `relay_bootstrap` identifies the current Agent, conversation, project binding, and wake-up.
- `chat_get_trigger` reads the exact message that caused this cycle.
- `agent_workspace_snapshot` supplies current-run project, team, Agent, and manager references for cross-conversation routing.
- Use unread and history tools for messages not already present; paginate older history deliberately.
- Use `chat_read_attachment` only with message and attachment references returned in this Run. Trigger images and PDFs may already be attached to the model input.

Temporary references are scoped capabilities, not stable IDs. Never invent, persist externally, or substitute a reference from another Run. Reading one conversation does not authorize sending to another.

Read [references/references-and-unread.md](references/references-and-unread.md) for unread cursor behavior, pagination, attachment limits, and cross-team routing preparation.
