# Relay references and unread state

## Wake-up versus unread

The wake-up only starts the Agent. It does not select a current conversation and does not inject a trigger message as the Agent's working context.

Good: call `chat_read_all_unread` at the start or retry, then decide independently whether each message needs a reply, a Todo, or no action. Read again when the latest runtime inbox snapshot reports new messages or unavailable status.

Bad: assume the newest message defines this Run, or limit work to the room that caused the wake-up.

## Global unread

`chat_read_all_unread` advances a read boundary inside this Run, so repeated calls return the next page instead of the same messages. It resets the runtime inbox signal only for the returned messages; new arrivals make the signal true again. Only the latest `chatos-runtime-state` system snapshot is current, and unavailable status never means the inbox is empty.

Durable read cursors advance atomically with successful `agent_cycle_complete`. An interrupted Run can safely reread unacknowledged messages on resume. Message references retain the authority needed by `chat_send_message`; the model does not need a conversation reference.

## Attachments

Read large text attachments in bounded pages. Do not claim an image, PDF, or non-UTF-8 file was fully interpreted when the tool returned only metadata or partial text.

## Routing preparation

Before contacting another team, use `agent_workspace_snapshot` to determine membership and its explicit project manager. Team members use the team route; outsiders open a direct conversation with the manager.
