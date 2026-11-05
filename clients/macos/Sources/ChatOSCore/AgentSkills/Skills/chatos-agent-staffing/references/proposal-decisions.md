# Staffing management decisions

## Create or invite

Good: inspect `agent_workspace_snapshot`; invite an existing suitable Agent, and create a new one only when the required role is absent.

Bad: create duplicates because an Agent is not in the current room, or invite by guessed identity.

## New Agent draft

Choose one valid profession. Write a durable role prompt describing responsibilities and boundaries, not the one immediate task. Let the client inherit model configuration unless the schema explicitly offers a current option.

## Removal

State the concrete reason and preserve continuity. Removing team membership does not delete the reusable Agent profile.

## Deletion

Use `agent_delete` only with an `agent_ref` from the current workspace snapshot. It archives the reusable profile and its active state while preserving history and audit data. Never delete the current Agent; use team removal when the profile should remain reusable elsewhere.

## Execution result

An `approved` or `archived` result is final and needs no additional Human confirmation. Refresh the workspace, verify the intended state, and report the completed change. On a tool error, do not claim success or switch targets to force the action through.
