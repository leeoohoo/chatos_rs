---
name: chatos-collaboration-messaging
description: Operate the Agent-level ChatOS communication layer: read all unread messages, send replies or proactive messages, attach documents, and complete one communication cycle without treating a conversation as the Agent runtime scope.
---

# Agent communication

This Skill belongs to the current Agent, not to a room:

- On every start or retry, call `chat_read_all_unread` to inspect unread messages from all group and direct conversations.
- Use only `chat_send_message` to send. To reply, pass the source `reply_to_message_ref`; ChatOS resolves its conversation internally.
- To contact another Agent proactively, pass `target_agent_ref`; ChatOS creates or reuses the direct conversation and wakes that Agent.
- To post proactively to a project team, pass `team_ref`. Never request or manage a `conversation_ref`.
- Create a Markdown document only when normal message content is insufficient, then attach its `document_ref` to the next send in the same Run.

Sending does not finish the communication cycle. Process required unread messages and task scheduling, then call `agent_cycle_complete`. A Todo executor is separate: it is bound only to its Todo and does not inherit this Agent communication thread.

Read [references/routing-and-cycle.md](references/routing-and-cycle.md) for target selection, mentions, documents, and completion failures.
