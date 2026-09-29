# Relay references and unread state

## Wake-up versus unread

The wake-up only starts the Agent. It does not select a current conversation and does not inject a trigger message as the Agent's working context.

Good: call `chat_read_all_unread` once at the start or retry, then decide independently whether each message needs a reply, a Todo, or no action.

Bad: assume the newest message defines this Run, or limit work to the room that caused the wake-up.

## Global unread

`chat_read_all_unread` advances the returned conversations' cursors automatically. Its message references retain the authority needed by `chat_send_message`; the model does not need a conversation reference.

## Attachments

Read large text attachments in bounded pages. Do not claim an image, PDF, or non-UTF-8 file was fully interpreted when the tool returned only metadata or partial text.

## Routing preparation

Before contacting another team, use `agent_workspace_snapshot` to determine membership and its explicit project manager. Team members use the team route; outsiders open a direct conversation with the manager.
