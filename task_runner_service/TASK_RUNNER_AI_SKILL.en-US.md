---
name: task-runner-ai-agent-en-us
description: English guide for AI agents using Task Runner MCP to externalize their own follow-through, execution, and review as an async continuation of their work, without making the user feel like they are operating a separate task system.
---

# Task Runner AI Agent Skill

Task Runner is an async channel that extends your own follow-through, execution, and review capacity.

From the user's perspective, this should feel like:

- you have already arranged the next steps
- you will keep moving the work forward and bring results back at the right moments
- you are using your own background execution chain, not throwing the work at a separate task system

## Default Mental Model

- Treat Task Runner as your own background execution lane, external working memory, and async extension layer, not as a product you need to explain to the user.
- Once the user hands the work to you, you are responsible for placing the follow-through into that lane; implementation output, validation evidence, and review conclusions come back to you and are then presented by you.
- In user-facing language, prefer phrasing like "next steps", "follow-through", or "I will bring the results back". Mention tasks, dependencies, or review structure explicitly only when the user actually needs that detail.

## Your Role

1. Translate the user's request into clear, executable, reviewable async work
2. Reuse and adjust existing tasks when possible instead of creating duplicates
3. Arrange dependencies, implementation phases, review phases, and capability boundaries
4. After task creation or adjustment succeeds, call `wait_for_task_completion`, then tell the user in natural language what follow-through you have arranged

Do not actively poll for progress.

## Core Rules

- Your goal is to continue moving the user's work forward, not to make them feel like they are operating a task-management product.
- In user-facing language, treat tasks as your own internal follow-through, next steps, or async execution chain. Do not foreground phrasing like "I created a task in the task system."
- After tasks are created or adjusted, call `wait_for_task_completion` once; completed results will be sent back later.
- If the user is following up, narrowing scope, adding constraints, or changing something already arranged, first use `list_tasks` / `get_task` / `get_task_dependency_graph` to identify the existing work, then decide whether to update it or create something new.
- If `update_task` or `set_task_prerequisites` can satisfy the request, adjust the existing task instead of creating a duplicate.
- If an existing task conflicts with the user's latest intent or has been replaced by the user's new request, call `cancel_task` for the task you judge to be conflicting or replaced, and provide a clear cancellation reason.
- Use the user's latest message plus the existing task details to decide which direct tasks should no longer continue; Task Runner automatically cascades cancellation to pending or running downstream tasks that depend on them.
- If the work will land in code, docs, config, scripts, prompts, pages, or other files, default to including a review step. Do not treat "implementation finished" as a real closure condition.
- Once task creation, updates, and dependency checks for the turn are complete, call `wait_for_task_completion`, then stop calling Task Runner tools.
- Do not ask the user or tool calls to carry model-selection fields; Task Runner binds an available model for the current user automatically. Use only real returned values for `task_id` and prerequisite IDs.
- Do not change task execution status. Task Runner maintains execution status.

## How To Write Better Tasks

Default toward tasks that are narrower, clearer, and easier to review. Avoid vague oversized tasks.

Try to make each task explicit about:

- what this step is meant to accomplish
- which modules, files, pages, APIs, or environments are in scope
- what artifact should come out of it: code changes, analysis, validation output, review findings, regression checks
- what should happen next once this step is complete

Recommended practice:

- make `title` a single action plus object
- use `objective` to define the intended outcome and done condition
- use `description` for context, constraints, risks, and review focus
- if the user gave substantial context, carry the important details into the task instead of leaving only an abstract goal

If the work is naturally multi-stage, split it instead of forcing it into one oversized task.

Default bias:

- clarify the work enough before arranging it
- when implementation and review can be separated, do not collapse them into one step
- aim for a task shape where a downstream agent can immediately see what the step owns, what it must produce, and how completion should be judged

## Default Decomposition Strategy

### Case 0: The user is following up on or changing existing work

Use `list_tasks` to find the relevant task. If the task ID is already known, use `get_task`.

If dependencies matter, use `get_task_dependency_graph` to inspect the chain.

Then:

- use `update_task` to change title, objective, input, tags, priority, or MCP capabilities
- use `set_task_prerequisites` to change prerequisite relationships
- use `cancel_task` when you judge that an already arranged pending or running item should no longer continue based on the user's latest message; the reason must explain why it no longer matches the user's current intent
- when cancelling a task that other tasks depend on, do not manually cancel every downstream task; Task Runner cascades cancellation to dependent pending or running tasks
- if an existing task already covers the new request, avoid creating another task and instead refine the existing arrangement

### Case 1: Read-only investigation, information gathering, or one-shot analysis

A single task is often enough. Use `create_task`.

Good fits:

- reading code and explaining the current behavior
- searching implementation and locating entry points
- collecting logs, config, or runtime information
- producing analysis without changing files

### Case 2: New feature work, code changes, file edits, or configuration changes

For execution-heavy work, the default should be an implementation task plus a review task.

In other words, do not stop at a single modification task unless a strong exception clearly applies.

You may skip a separate review task only in cases like:

- the change is extremely small and purely mechanical, such as formatting or wording, with no behavior impact
- the user explicitly wants investigation or read-only analysis and is not asking for implementation
- there is already a downstream review / synthesis / acceptance task that clearly covers this change

If you are unsure whether review is needed, assume that it is.

Recommended split:

1. Implementation task: complete the requested change and necessary verification
2. Review task: depend on the implementation task and independently check quality, impact, regression risk, tests, and acceptance coverage

The review task should focus on:

- whether the request was actually satisfied
- whether the change scope is appropriate
- whether obvious regressions or omissions remain
- whether commands, tests, UI checks, logs, or other validation steps were adequately covered
- whether more follow-up changes are still needed

If the work changes visible behavior, the review task should usually include UI or behavior verification too.

Treat "review has been arranged" as part of a complete plan, not as an optional bonus.

### Case 3: Naturally multi-stage work

If the request has natural phases, prefer `create_tasks_with_prerequisites` so the whole chain is created together.

Common patterns:

- investigate, then implement, then review
- collect logs, then analyze root cause, then fix, then re-check
- finish multiple subtasks, then use a final synthesis task

Rules:

- each new task gets a temporary `client_ref`
- dependencies within the same request use `prerequisite_refs`
- after creation, rely only on real `task_id` values

### Case 4: The dependency already exists

Obtain the real task IDs first, then pass them through `prerequisite_task_ids` on `create_task`.

## Pairing Implementation With Review

If the work will modify code, docs, config, scripts, prompts, pages, or other files, default toward:

- an implementation task focused on making the change
- a review task focused on independently checking whether the change is actually correct

Hard constraint:

- Whenever a task enables `CodeMaintainerWrite`, it must also enable `CodeMaintainerRead`. Do not create code tasks that have write tools but no read tools.

Usually:

- implementation leans toward `CodeMaintainerRead` + `CodeMaintainerWrite`, and often also `TerminalController` or `BrowserTools`
- review leans toward `CodeMaintainerRead`, and often also `TerminalController` or `BrowserTools`

The review task should not merely repeat the implementation task. It should explicitly own validation, audit, regression checking, and omission detection.

It is usually helpful for the review objective to explicitly include:

- whether the implementation truly satisfies the original request
- whether any half-finished edges, scope gaps, or hidden regressions remain
- whether the available validation evidence is strong enough to support delivery confidence

## Choosing Builtin MCP Capabilities

Use `enabled_builtin_kinds` to define what a task may use during execution.

Principle: enable what is genuinely needed, but do not starve the task of required capabilities.

Common capability guide:

- `CodeMaintainerRead`: inspect code, search implementation, understand behavior
- `CodeMaintainerWrite`: edit code, create patches, fix issues
- `TerminalController`: run commands, compile, test, inspect output
- `BrowserTools`: open pages, inspect UI, capture screenshots
- `WebTools`: search public information and read webpages
- `RemoteConnectionController`: connect to remote servers
- `TaskManager`: split and track subtasks during execution
- `Notepad`: record observations and intermediate findings
- `UiPrompter`: ask for user input during execution

Recommended combinations:

- code investigation: `CodeMaintainerRead`
- code fix: `CodeMaintainerRead` + `CodeMaintainerWrite` + `TerminalController`
- frontend change: `CodeMaintainerRead` + `CodeMaintainerWrite` + `TerminalController` + `BrowserTools`
- frontend review: `CodeMaintainerRead` + `TerminalController` + `BrowserTools`
- remote troubleshooting: `RemoteConnectionController` + `TerminalController`

## Prerequisite Rules

- a task may have multiple prerequisites
- all prerequisites must finish before the current task can run
- dependencies must never form a cycle
- when the current task runs, Task Runner automatically injects prerequisite results and process logs into the prompt

So:

- if the request is inherently staged, model it explicitly as dependent tasks
- if implementation should be followed by a real review, model that review dependency explicitly
- do not cram obviously separate phases into one overloaded task

## How To Talk To The User

Your wording should make this feel like your own follow-through, not like a separate system is taking over.

Prefer language like:

- "I split this into two next steps: first the change itself, then an independent review."
- "I arranged implementation and validation as separate stages so the result can come back more clearly."
- "I split the investigation, change, and review so each stage has a cleaner output and check."

Avoid language like:

- "I created a task for you"
- "the task system will do it for me"
- "you are now using a task system"
- "I will poll the task result"

## How To Close The Turn

Once task creation or update succeeds, call `wait_for_task_completion`.

Then reply with a concise summary covering:

- what follow-through you arranged
- the expected order of execution
- whether implementation and review were separated
- what results are expected to come back

Do not:

- say you are executing everything in real time
- say you will keep polling
- reveal tool-by-tool traces
- dump internal task IDs unless the user explicitly asks

## Recommended Reply Style

Example 1:

"I split this into three follow-through stages: investigate the current state, make the change, and then run an independent review focused on regression risk and acceptance coverage. It will move in that order, and I will keep bringing the results from each stage back to you."

Example 2:

"I arranged this as implementation plus review. The first step handles the code and file changes, and the second step independently checks quality, validation coverage, and likely omissions so the result I bring back is more reliable."

## Do Not Do These

- do not call any Task Runner tools after `wait_for_task_completion`
- do not present internal execution traces as the final answer
- do not promise to wait synchronously inside the current request until everything finishes
- do not repeatedly inspect tasks just to confirm completion; results will be sent back by Task Runner

## One-Line Principle

You are externalizing your own internal execution chain into async follow-through, not handing the user off to a "task system."
