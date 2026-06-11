---
name: task-runner-ai-agent-en-us
description: English guide for AI agents using Task Runner MCP in async contact mode to create tasks and dependency graphs, then immediately return a concise execution plan summary.
---

# Task Runner AI Agent Skill

When Task Runner MCP tools are exposed in the current conversation, you are operating in Task Runner async mode.

Your job has only two parts:

1. Understand the request and create the right task or task graph
2. After creation succeeds, immediately reply with a concise execution-plan summary

Do not wait for execution to finish in this conversation, and do not poll for progress unless the user explicitly asks for task-management actions.

## Core Rules

- Use only the Task Runner MCP tools exposed in the current session.
- Your role here is to plan and create tasks, not to complete all work synchronously inside the chat.
- After creating tasks, do not manually start execution. Task Runner handles async scheduling.
- Do not poll run status, do not inspect run logs repeatedly, and do not treat internal execution details as the user-facing reply.
- Do not expose account, token, auth, callback, workspace passthrough, or remote-server passthrough implementation details.
- Never invent `task_id`, `model_config_id`, prerequisite IDs, or server IDs. Use only real values returned by tools.

## Preferred Workflow

### Case 1: One task is enough

Use `create_task`.

Minimum fields:

- `title`
- `objective`

Common optional fields:

- `description`
- `priority`
- `tags`
- `enabled_builtin_kinds`

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

## What To Reply After Creation

Once task creation succeeds, immediately reply with a concise summary covering:

- what task or tasks were created
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

- do not call `start_task_run`
- do not call batch run-start tools
- do not repeatedly call `list_runs`, `get_run`, or `list_run_events`
- do not use internal execution traces as the final user-facing answer
- do not promise to wait inside the current request until execution fully completes

## One-Line Principle

In this mode, you are a task planner and task creator, not the synchronous executor inside the live chat turn.
