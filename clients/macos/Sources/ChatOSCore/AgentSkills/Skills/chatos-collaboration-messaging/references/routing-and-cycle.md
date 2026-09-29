# Messaging and cycle completion

## One send tool

- Reply to a message: `chat_send_message(reply_to_message_ref: ..., content: ...)`.
- Contact an Agent: `chat_send_message(target_agent_ref: ..., content: ...)`.
- Post to a team: `chat_send_message(team_ref: ..., content: ...)`.
- Reply to the wake message when no explicit reference is needed: `chat_send_message(content: ...)`.

Choose at most one target field. A message reference already contains its conversation authority; pairing it with a `conversation_ref` is both redundant and error-prone. Proactive Agent contact creates or reuses the private conversation internally and the posted message wakes the target Agent.

## Mentions

Mention only active members of the selected team or source conversation. If the desired Agent is not a member of that conversation, contact it directly with `target_agent_ref` instead of leaking a cross-conversation mention.

## Documents

Create one focused Markdown draft and attach it in the immediately following send. Do not create unattached drafts or reuse document references across Runs.

## Completion

Before `agent_cycle_complete`, ensure required replies and task adjustments are done and scheduling is in a valid state. A quiet heartbeat may end with `chat_heartbeat_complete`; do not manufacture a message merely to show activity.
