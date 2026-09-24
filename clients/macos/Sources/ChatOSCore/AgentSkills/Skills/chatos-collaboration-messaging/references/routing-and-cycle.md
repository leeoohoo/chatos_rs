# Messaging routes and cycle completion

## Current conversation

Good: use `chat_send_message` with a returned message reference when a threaded reply matters.

Bad: use a global inbox reference in a current-room tool or send several fragments that should be one coherent response.

## Team handoff

Good: include the Human's original objective, source, scope, expected output, acceptance suggestion, constraints, and key URLs when handing work to a project manager.

Bad: say a Todo exists before the project manager actually creates it.

## Mentions

Mention only the members who need to act. A display name is not a substitute for a current-run `agent_ref`.

## Documents

Good: create one focused Markdown draft and attach it in the immediately following send.

Bad: create an unattached draft, reuse a document reference across Runs, or use a document to hide an otherwise empty status update.

## Completion

Before `agent_cycle_complete`, ensure necessary replies and task adjustments are done and scheduling is in a valid state. A quiet heartbeat ends with `chat_heartbeat_complete`; do not manufacture a message merely to show activity.
