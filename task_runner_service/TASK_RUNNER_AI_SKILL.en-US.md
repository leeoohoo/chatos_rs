---
name: task-runner-ai-agent-en-us
description: English guide for AI agents using Task Runner MCP to create, inspect, execute tasks, select builtin MCP capabilities, manage prerequisite task dependencies, and read run results.
---

# Task Runner AI Agent Skill

Use this guide when Task Runner MCP tools are available in the current session.

## Core Rules

- Operate only on tasks visible to the current agent. If `list_tasks` / `get_task` cannot see a task, do not assume you can reference it.
- Never invent `task_id`, `run_id`, `model_config_id`, or prerequisite task IDs. Use only real IDs returned by tools.
- When creating a task, pass only business fields required or allowed by the tool schema.
- Creating a task does not necessarily execute it. To run it now, call `start_task_run` or `batch_start_task_runs`.
- Confirm destructive or irreversible actions, such as deleting tasks, batch status changes, or canceling runs, when user intent is not explicit.

## Common Workflows

### 1. Inspect Existing Tasks

Use `list_tasks` first, then `get_task` for details.

```json
{
  "status": "pending",
  "keyword": "deploy",
  "limit": 20
}
```

Supported filters include `status`, `keyword`, `tag`, `model_config_id`, `scheduled_only`, `parent_task_id`, `source_run_id`, and `limit`.

### 2. Create A Standard Task

`create_task` only requires `title` and `objective`.

```json
{
  "title": "Investigate order sync failures",
  "objective": "Find the direct cause of the order sync failures and report evidence, impact, and recommended fixes.",
  "description": "The user reported that some orders from the last 2 hours did not sync to the downstream system.",
  "priority": 50,
  "tags": ["orders", "incident"],
  "input_payload": {
    "time_range": "last_2_hours",
    "systems": ["order-service", "downstream-sync"]
  }
}
```

Field guide:

- `title`: Short human-readable title.
- `objective`: The result that must be true when the task is done. Make it verifiable.
- `description`: Background, constraints, and extra context.
- `input_payload`: Structured inputs, log snippets, business parameters, or external references.
- `priority`: Higher number means higher priority.
- `tags`: Useful for search and grouping.
- `default_model_config_id`: Pass only when a specific model is required. The value must come from `list_model_configs` / `get_model_config`.
- `schedule`: Pass only when the user explicitly requests delayed, timed, or recurring execution.
- `enabled_builtin_kinds`: Builtin MCP capabilities allowed during task execution.
- `prerequisite_task_ids`: Real task IDs that must complete successfully before this task runs.

Do not add system-internal fields outside the `create_task` tool schema.

### 3. Select Builtin MCP Capabilities For Execution

If you are unsure what is currently available, call `list_mcp_builtin_catalog`. Use `enabled_builtin_kinds` when creating a task.

```json
{
  "title": "Fix login page button layout",
  "objective": "Fix the mobile login button layout issue and report verification results.",
  "enabled_builtin_kinds": [
    "CodeMaintainerWrite",
    "TerminalController",
    "BrowserTools"
  ]
}
```

Capability guide:

- `CodeMaintainerRead`: Read-only repository inspection, code search, implementation discovery, review.
- `CodeMaintainerWrite`: Edit repository files, create patches, fix defects.
- `TerminalController`: Run commands, build checks, scripts, and inspect terminal output.
- `TaskManager`: Split work, track subtasks, and maintain execution TODOs.
- `Notepad`: Store plans, observations, and intermediate conclusions in long tasks.
- `AgentBuilder`: Maintain agent config, capability descriptions, and build materials.
- `UiPrompter`: Ask the user for input, choices, or confirmation during execution.
- `RemoteConnectionController`: Work with remote machines registered in Task Runner server settings.
- `WebTools`: Search external information, read webpages, and verify public facts.
- `BrowserTools`: Open and operate webpages, inspect UI, capture screenshots, and debug frontend behavior.

Selection guidance:

- For read-only investigation, use `CodeMaintainerRead`.
- For code changes, use `CodeMaintainerWrite`, usually with `TerminalController` for verification.
- For frontend UI behavior, add `BrowserTools`.
- For remote logs or deployment environments, add `RemoteConnectionController`.
- For mid-run user decisions, add `UiPrompter`.
- Do not enable every capability just in case.

### 4. Create A Task With Existing Prerequisites

If prerequisite tasks already exist, first obtain their real `task_id` values with `list_tasks` or `get_task`, then create the dependent task.

```json
{
  "title": "Produce release risk conclusion",
  "objective": "Use prerequisite check results to decide whether release is safe, list risks, and propose rollback guidance.",
  "prerequisite_task_ids": ["task_real_id_from_list_or_create"]
}
```

Rules:

- `prerequisite_task_ids` must contain real task IDs only.
- A task may have multiple prerequisites. The current task runs only after all prerequisites finish successfully.
- A task cannot depend on itself and dependencies cannot form a cycle.
- When running the current task, the system executes or waits for prerequisites first and injects prerequisite results and process logs into the current task global prompt.
- If any prerequisite fails, the current task is blocked or fails. Do not report it as complete.

### 5. Create A New Task Graph In One Call

When new prerequisite tasks do not yet have real IDs, use `create_tasks_with_prerequisites`. Each task uses `client_ref` as a temporary reference within this one request, and `prerequisite_refs` points to other tasks created in the same request.

```json
{
  "tasks": [
    {
      "client_ref": "collect_logs",
      "title": "Collect sync path logs",
      "objective": "Collect error logs from the order sync path for the last 2 hours and summarize key anomalies.",
      "enabled_builtin_kinds": ["TerminalController", "RemoteConnectionController"]
    },
    {
      "client_ref": "inspect_code",
      "title": "Inspect order sync code",
      "objective": "Read order sync code and identify logic that could cause missed syncs.",
      "enabled_builtin_kinds": ["CodeMaintainerRead"]
    },
    {
      "client_ref": "diagnose",
      "title": "Diagnose order sync incident",
      "objective": "Combine log and code findings to report root cause, evidence, and recommended fixes.",
      "prerequisite_refs": ["collect_logs", "inspect_code"]
    }
  ]
}
```

The tool returns the real `task_id` for each `client_ref`. After that, use only the real `task_id`; do not treat `client_ref` as a task ID.

### 6. Update Or Inspect Prerequisites

Replace the direct prerequisites for an existing task:

```json
{
  "task_id": "task_current",
  "prerequisite_task_ids": ["task_a", "task_b"]
}
```

Clear prerequisites with an empty array:

```json
{
  "task_id": "task_current",
  "prerequisite_task_ids": []
}
```

After changes, call `get_task_dependency_graph` to inspect direct dependencies, transitive prerequisites, blockers, and `ready`.

### 7. Execute Tasks And Read Results

Run one task now:

```json
{
  "task_id": "task_current"
}
```

Run with a specific model:

```json
{
  "task_id": "task_current",
  "model_config_id": "model_config_id_from_list_model_configs"
}
```

If the task has prerequisites, start the current task. The system handles prerequisite execution order and automatically includes prerequisite results and process logs in the current task prompt.

Process logs are maintained by Task Runner's internal executor. External agents do not need to and cannot write process logs directly.

Inspect runs:

- `list_runs`: List runs by task, status, or model.
- `get_run`: Read one run's details and output.
- `list_run_events`: Inspect execution events, tool calls, and failure points.
- `cancel_run`: Cancel a queued or running run.
- `retry_run`: Create a retry run based on a previous run.

### 8. Handle UI Prompts During Execution

When a task enables `UiPrompter`, execution may create prompts that need user input.

- `list_prompts`: Find prompts for a task or run.
- `get_prompt`: Read prompt details.
- `submit_prompt`: Submit user-provided values or selections.
- `cancel_prompt`: Cancel a prompt when cancellation is allowed.

Do not fabricate user confirmation. If a choice is needed, explain the options and impact clearly.

### 9. Read Task Memory

- `get_task_memory_context`: Read composed Memory Engine context and thread summary for a task.
- `list_task_memory_records`: List persisted records for the task thread.
- `summarize_task_memory`: Trigger one repair summary. This is for repair or manual cleanup, not the normal execution path. Use it only when the user asks or the context clearly needs repair.

## Model Configs

Regular agents may use:

- `list_model_configs`
- `get_model_config`

Admin-only operations:

- `create_model_config`
- `update_model_config`
- `delete_model_config`
- `test_model_config`

If the user asks for a specific model, call `list_model_configs` to obtain the real `model_config_id`, then pass it to `create_task.default_model_config_id` or `start_task_run.model_config_id`. Do not guess IDs.

## Decision Template

For each user request, decide in this order:

1. Is the user asking about existing tasks? Use `list_tasks` / `get_task`.
2. Is the user asking to create work? Call `create_task` with only needed fields.
3. Does execution require code, terminal, browser, remote server, or web capabilities? Select the smallest useful `enabled_builtin_kinds`.
4. Does the task depend on other task results? Use `prerequisite_task_ids` for existing tasks, or `create_tasks_with_prerequisites` for new tasks in the same graph.
5. Did the user ask to run it now? After creation, call `start_task_run`.
6. Does the user need a result report? Use `get_run` and `list_run_events` to summarize facts. Do not fill gaps from imagination.

## Reporting Back To The User

When useful, report:

- Created task titles and real `task_id` values.
- Whether prerequisites were configured, and their task IDs.
- Whether execution started, and the real `run_id`.
- Current status, failure reason, or user confirmation needed next.
