---
name: chatos-collaboration-messaging
description: Send scoped ChatOS replies, direct or team messages, attach generated Markdown documents, advance read state, and complete communication cycles without misrouting work or claiming unconfirmed outcomes.
---

# Collaboration messaging

Send to the conversation that owns the work and use only current-run references:

- Reply in the current conversation with `chat_send_message`.
- Reply to a global unread item with `chat_inbox_send` when its source conversation remains accessible.
- For another project team, use `chat_team_send` when you are a member; otherwise open a direct conversation with its explicit project manager and use `chat_direct_send`.
- Create a Markdown document only when the content is too large or structured for a normal message, then attach its `document_ref` in the next send within the same Run.

Sending a message does not finish the cycle. Process required unread state and scheduling, then call `agent_cycle_complete`. During a quiet heartbeat with nothing actionable, use `chat_heartbeat_complete` instead of posting noise.

Read [references/routing-and-cycle.md](references/routing-and-cycle.md) for reply targets, mentions, documents, manager handoff, and completion failures.
