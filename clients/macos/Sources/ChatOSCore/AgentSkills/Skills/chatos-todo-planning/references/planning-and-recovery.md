# Todo planning and recovery

## Create complete work

Good: preserve the Human's source message, define observable outputs and acceptance, assign a current member, and select the minimum execution capabilities.

Bad: create “handle this” tasks, omit acceptance, or grant terminal and every Plugin by default.

Good: split technical validation, coordination, Human research, real-person testing, approval, and deployment when they have different owners or execution conditions. A completed predecessor may produce the exact input for a successor.

Bad: assign one member a mixed contract that includes work they cannot perform, then surface the predictable handoff as a Human blocker.

## Capability selection

Use project read for inspection, project write for file mutation, terminal for commands or remote repository operations, and requirement survey only when Human input must be gathered. Plugin hints must come from current execution options.

## Dependencies

Good: obtain fresh dependency options and encode only direct prerequisites.

Bad: create cross-team, circular, self, duplicate, or speculative dependencies.

## Update and reorder

Update structure or ordinary blocked state only from current facts. Reorder independent work deliberately; do not use order to pretend a blocked dependency is ready.

Ordinary blockers belong to the project manager: inspect evidence, correct scope, split or reassign work, create a successor or investigation Todo, and resume work when the dependency is real and available. Escalate to the Human only when the remaining action genuinely requires Human authority, budget, credentials, product direction, or an external act the team cannot perform.

## Ambiguous side effects

When a task is in `needsReview` because a write or billable result is unknown, do not reset it to pending. A Human must choose the explicit retry action in the run details.

## Start next

Call `todo_schedule_state` before finishing a communication cycle. Start the next ready task when appropriate, or leave the scheduler idle only when no ready work exists or another executor is active.
