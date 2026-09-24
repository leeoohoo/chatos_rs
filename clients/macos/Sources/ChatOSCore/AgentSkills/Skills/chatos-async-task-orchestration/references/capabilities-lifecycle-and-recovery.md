# Capabilities, lifecycle, and recovery

## Minimal capability selection

- Project inspection and search: `CodeMaintainerRead`.
- Project file creation, modification, or deletion: `CodeMaintainerWrite`; read access is added by policy.
- Commands, Git, dependency installation, tests, type checks, builds, or runtime verification: `TerminalController`; read access is added by policy.
- Existing surveys, Human answers, resolutions, execution plans, or project task state: `RequirementSurveyRead`.
- Survey creation or writing a submitted survey's resolution and plan: `RequirementSurveyWrite`; read access is added by policy.

Combine capabilities only when the task truly needs each one. Set execution intent for commands or file changes; pure reading and analysis do not require it. Choose model configuration only from the current tool schema.

## Task shape

Use one task when implementation and verification belong to one deliverable. Split work only when the current run exposes multi-task creation and the stages have real prerequisite edges. Never encode secrets in titles, objectives, inputs, or tags.

## Surveys

Require the worker to check existing surveys before creating one. A pending survey cannot be resolved by the Agent. A written resolution and execution plan complete the survey workflow, not the later implementation.

## Recovery

- If the user changes direction, cancel obsolete pending or running work with a concrete reason before creating replacement work.
- If a capability is unavailable, create a narrowly scoped investigation only when it can establish the missing fact. Report a blocker only after the tool returns concrete unavailability.
- Do not restart completed historical work or poll after the handoff signal.
