# Relay references and unread state

## Trigger versus unread

Good: read the trigger first, then check unread only when the cycle may depend on additional messages.

Bad: assume the newest message is the trigger or repeatedly scan all history.

## Global unread

`chat_read_all_unread` advances the returned conversations' cursors automatically. Decide whether each item needs a reply, a Todo, or no action before sending anything.

`chat_read_unread` does not confirm the current-room cursor. After processing, use `chat_mark_read` with the exact returned message reference.

## Older history

Start from the recent page. Continue with `next_before_message_ref` only when older context is necessary. Do not infer that omitted history does not exist.

## Attachments

Read large text attachments in bounded pages. Do not claim an image, PDF, or non-UTF-8 file was fully interpreted when the tool returned only metadata or partial text.

## Routing preparation

Before contacting another team, use `agent_workspace_snapshot` to determine membership and its explicit project manager. Team members use the team route; outsiders open a direct conversation with the manager.
