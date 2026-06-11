# Task Runner Async Feedback Fix Log

## Scope

This record tracks the issues reported during the Chatos + Task Runner async workflow rollout. Each item must be closed by code changes and compile verification, not by hiding UI or relying on prompts only.

## Issues

### 1. Completed tasks still cause repeated task creation

- Symptom: after Task Runner tasks are completed multiple times, Chatos still keeps asking the model to create more similar tasks.
- Evidence:
  - Chatos request logs still show Task Runner create tools exposed to the model.
  - The same user intent created multiple similar tasks.
  - Task completion callbacks failed to persist as assistant messages, so later conversation context lacks the completion result.
- Fix plan:
  - Add backend idempotency for Task Runner tasks created from the same `source_user_message_id`.
  - Keep Chatos async planner behavior strict: after task creation, return a summary and stop tool use in that model loop.
  - Ensure callback results become normal visible assistant messages in Chatos history.
- Status: Fixed. Task Runner MCP now reuses existing tasks for the same Chatos user message in async planner mode. Compile check passed.

### 2. Task completion callback does not reach the user

- Symptom: Task Runner reports task completion, but the Chatos user does not receive a message.
- Evidence:
  - Task Runner callback request returns `500 Internal Server Error`.
  - Error detail: `tenant_id is required`.
- Root cause:
  - The callback path writes Chatos messages through a tenant-less session/message lookup path. Memory Engine rejects the write because it requires `tenant_id`.
- Fix plan:
  - Use the original user message/session owner as the tenant source.
  - Persist the callback assistant message through a tenant-aware/session-aware path.
  - Push the callback event to the frontend only after the message history write succeeds.
- Status: Fixed. Chatos callback lookup/upsert now uses the source session and tenant-aware record access. Compile check passed.

### 3. Callback content should contain only the final task result

- Symptom: callback payload/message included execution process and tool details that should not be sent to the user.
- Fix plan:
  - Keep callback payload and Chatos callback message limited to the final task result/summary.
  - Do not include process log or tool execution trace.
- Status: Fixed earlier and compile check passed in this round.

### 4. Message status must be persisted by backend, not frontend cache

- Symptom: after page refresh, message status can fall back to `pending`.
- Expected:
  - User message is `pending` when sent.
  - It becomes `processing` when Chatos backend starts the model loop.
  - It becomes `completed` when Chatos finishes creating the Task Runner task and returns the plan summary.
  - Later Task Runner completion callback creates an additional assistant message with the final task result.
- Fix plan:
  - Keep status in message metadata/history.
  - Avoid relying on transient frontend state for async task status.
- Status: Fixed. `processing` and `completed` status updates now use session-aware message writes. Compile check passed.

### 5. Chatos-side async mode should not expose execution/polling behavior to the model

- Symptom: the model observes task state and creates follow-up duplicate tasks instead of stopping after creation.
- Expected:
  - The model only creates the async task and returns an immediate plan summary.
  - Background scheduler in Task Runner executes the task later.
  - The model should not execute tasks or poll task completion from Chatos.
- Fix plan:
  - Do not expose Task Runner execution tools to Chatos async contacts.
  - Minimize query/read tools exposed in this mode.
  - Update skill/prompt wording to state: created tasks are executed automatically in the background.
- Status: Fixed. Existing tool profile already hides execution tools in Chatos async mode; duplicate creates are now guarded at backend level. Compile check passed.

### 6. AI-created async tasks need explicit model and MCP capability selection

- Symptom: when multiple Task Runner model configs exist, AI may not fill/select the intended task model.
- Expected:
  - `default_model_config_id` is required and described with configured usage scenarios.
  - `enabled_builtin_kinds` is required and described with capability usage scenarios.
- Status: Fixed earlier and compile check passed in this round.

### 7. Chatos async mode still needs task inspection and adjustment tools

- Symptom: exposing only create tools is too restrictive. If the user follows up with changed requirements, the AI cannot inspect or adjust existing tasks.
- Expected:
  - Chatos async mode can inspect owned tasks through task read tools.
  - Chatos async mode can update task definitions and prerequisite relationships.
  - Chatos async mode still cannot execute tasks or poll run execution details.
- Status: Fixed. The async profile now exposes task read/adjustment tools while keeping run execution tools blocked. Compile check passed.

### 8. Chatos-side Task Runner MCP calls must still be stored in Memory Engine

- Symptom: Memory Engine only showed the user message for a Chatos async turn, but not the assistant tool-call message where Chatos called Task Runner MCP.
- Root cause:
  - Chatos async plan mode returned early when the assistant response contained `tool_calls`.
  - This skipped persistence of the assistant tool-call message.
  - The async plan execution loop also disabled `persist_tool_messages`, so Task Runner MCP tool result messages were not written to Memory Engine.
- Expected:
  - Chatos-side calls to Task Runner MCP must be persisted as assistant/tool messages in Memory Engine.
  - Task Runner completion callbacks must still send only the final task result back to Chatos, without Task Runner internal execution tool traces.
- Status: Fixed. Assistant tool-call messages in `task_runner_async_plan` mode are now persisted with `task_runner_async.message_kind = "tool_call"`, tool result messages are persisted again through the normal `save_tool_results` path, while plan summaries remain `message_kind = "plan_summary"` and Task Runner callbacks still omit process logs.

### 9. User-message async status remains processing after refresh

- Symptom: after hard refresh, the original Chatos user message can still show `processing` even though Chatos already returned the plan summary and Task Runner later completed the background task.
- Root cause:
  - The Chatos status write paths were silently ignoring failures, so missed `processing -> completed` writes were invisible in logs.
  - Task Runner terminal callbacks updated task tracking sets but did not force the source Chatos user message back to `completed`.
- Expected:
  - `overall_status` represents Chatos handling of the user message, not the background Task Runner execution state.
  - Background task completion should create a separate assistant result message and also act as a final `completed` status fallback for the source user message.
- Status: Fixed. Chatos now logs failed status writes, terminal Task Runner callbacks mark the source user message `task_runner_async.overall_status = "completed"`, and compact history display repairs older persisted `processing` metadata when the turn already has a plan summary or terminal task tracking.
