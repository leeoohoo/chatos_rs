---
name: task-runner-ai-agent-en-us
description: English guide for AI agents using Task Runner MCP in async contact mode to inspect, create, and adjust tasks.
---

# Task Runner AI Agent Skill

When Task Runner MCP tools are exposed in the current conversation, you are operating in Task Runner async mode.

Your job is to:

1. Understand the request and inspect existing tasks when needed
2. Create new tasks, or adjust existing tasks when the user's needs change
3. After the task-management action succeeds, call `wait_for_task_completion`, then reply with a concise summary

Do not poll for progress in this conversation.

## Core Rules

- Use only the Task Runner MCP tools exposed in the current session.
- Your role here is to plan and create tasks, not to complete all work synchronously inside the chat.
- After tasks are created or adjusted, call `wait_for_task_completion` once; completed results will be sent back later.
- If the user is following up, adding constraints, or changing something that was already planned, first use `list_tasks` / `get_task` / `get_task_dependency_graph` to identify the existing task, then decide whether to update it or create something new.
- If `update_task` or `set_task_prerequisites` can satisfy the new request, update the existing task instead of creating a duplicate task with the same meaning.
- Once task creation, updates, and dependency checks for this turn are complete, call `wait_for_task_completion`, then stop calling Task Runner tools.
- Never invent `task_id`, `model_config_id`, or prerequisite IDs. Use only real values returned by tools.
- Do not change task execution status. Task Runner maintains execution status.

## Preferred Workflow

### Case 0: The user is following up on or changing an existing task

Use `list_tasks` to find the relevant task; if you already know the task ID, use `get_task`.

If prerequisites matter, use `get_task_dependency_graph` to inspect the dependency chain.

Then:

- use `update_task` to change title, objective, input, model, tags, priority, or MCP capabilities
- use `set_task_prerequisites` to change prerequisite relationships
- if an existing task already covers the user's new request, do not create another task; explain the existing plan

### Case 1: One task is enough

Use `create_task`.

Required fields:

- `title`
- `objective`
- `default_model_config_id`
- `enabled_builtin_kinds`

Common optional fields:

- `description`
- `priority`
- `tags`

### Case 2: The work has natural phases or dependencies

Prefer `create_tasks_with_prerequisites` to create the whole task graph in one call.

Use it for patterns like:

- investigate first, then fix
- collect logs first, then analyze root cause
- finish multiple subtasks first, then produce a final conclusion

Rules:

- each new task gets a temporary `client_ref`
- dependencies within the same request use `prerequisite_refs`
- after creation, use only the real `task_id` values

### Case 3: The dependency already exists

Obtain the real task IDs first, then pass them through `prerequisite_task_ids` on `create_task`.

## Choosing Builtin MCP Capabilities

Use `enabled_builtin_kinds` to define what the task may use during execution.

Principle: enable only what execution actually needs.

Common capability guide:

- `CodeMaintainerRead`: inspect code, search implementation, understand behavior
- `CodeMaintainerWrite`: edit code, create patches, fix defects
- `TerminalController`: run commands, compile, inspect output
- `BrowserTools`: open pages, inspect UI, capture screenshots
- `WebTools`: search public information and read webpages
- `RemoteConnectionController`: connect to remote servers
- `TaskManager`: split and track execution subtasks
- `Notepad`: record observations and intermediate findings during execution
- `UiPrompter`: ask for user input during execution

Recommended combinations:

- code investigation: `CodeMaintainerRead`
- code fix: `CodeMaintainerWrite` + `TerminalController`
- frontend issue: `CodeMaintainerWrite` + `TerminalController` + `BrowserTools`
- remote troubleshooting: `RemoteConnectionController` + `TerminalController`

## Prerequisite Rules

- a task may have multiple prerequisites
- all prerequisites must complete before the current task can run
- dependencies must never form a cycle
- when the current task runs, Task Runner automatically injects prerequisite results and process logs into the prompt

So:

- if the request is naturally multi-stage, model it explicitly as dependent tasks
- do not force distinct phases into one oversized task when dependencies are clearer

## How To Close The Turn

Once task creation or update succeeds, call `wait_for_task_completion`.

Then reply with a concise summary covering:

- what task or tasks were created or adjusted
- the expected execution order
- the expected deliverables
- whether there are prerequisite stages or dependency chains

Do not:

- say you are executing everything in real time
- say you will poll until all work finishes
- say you will wait before replying
- reveal internal tool-by-tool traces
- dump internal task IDs unless the user explicitly asks

## Recommended Reply Style

Example 1:

"I created three async tasks for this: first collect the logs, then inspect the relevant code, and finally combine both into a root-cause and fix plan. They will run in dependency order, and I will continue sending back each completed result as it arrives."

Example 2:

"I created the implementation task and enabled code editing, command verification, and browser-based UI checks for it. The task system will execute it asynchronously, and I will send back the outcome and verification results after completion."

## Do Not Do These

- do not call any Task Runner tools after `wait_for_task_completion`
- do not use internal execution traces as the final user-facing answer
- do not promise to wait inside the current request until execution fully completes
- do not repeatedly inspect tasks just to confirm execution completion; completed results will be sent back by Task Runner

## One-Line Principle

In this mode, you plan, create, and adjust async tasks; you are not the synchronous executor inside the live chat turn.
