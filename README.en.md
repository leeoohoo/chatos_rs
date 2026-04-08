# Agent Stack

## Overview

Agent Stack is an AI agent infrastructure project for engineering collaboration, persistent context management, and asynchronous task execution.

Its purpose is to turn AI from a one-shot chat interface into a reliable operational system: users can talk to AI contacts, AI can plan and create structured tasks, tasks can run with controlled tool access, and results can be returned through a unified interaction model.

## Mission

The project is built around a simple idea:

AI should not only answer questions. It should be able to participate in real work.

That means the system must support:
- long-lived context instead of session-only memory
- explicit task planning instead of hidden reasoning only
- controlled tool execution instead of ad hoc integrations
- asynchronous delivery instead of blocking every interaction on completion
- interoperable APIs instead of product-specific lock-in

## What Problems It Solves

Traditional chat-centric AI products often struggle in engineering and operations scenarios:
- conversation history becomes too long and too expensive
- important facts, decisions, and preferences are easily lost
- tool usage is hard to coordinate, audit, and reuse
- task creation and task execution are not modeled as first-class workflow concepts
- external systems need custom integration work for every connection

Agent Stack addresses these issues through a modular architecture that separates workspace interaction, orchestration, memory, task lifecycle, IM delivery, and gateway compatibility.

## Core Capabilities

- Persistent memory: layered summaries, rollups, and memory consolidation preserve key facts without replaying full history
- IM-style collaboration: users interact with AI contacts through a messaging-oriented experience rather than a raw tool stream
- Structured task planning: natural-language intent can be converted into reviewable tasks and task graphs
- Async task execution: confirmed tasks execute in the background and report results back later
- Tool-scoped runtime: execution can use builtin capabilities such as read, write, terminal, remote, notepad, and UI prompting within controlled runtime scope
- OpenAI-compatible access: existing SDKs and clients can integrate through a familiar API surface

## End-to-End Workflow

1. A user sends a message to an AI contact in the workspace.
2. The orchestration layer assembles runtime context from history, summaries, memory, and authorized resources.
3. The model either responds directly or creates one or more tasks for review.
4. After confirmation, the task system executes those tasks asynchronously with the required tools and assets.
5. Results, summaries, and follow-up knowledge are written back into the system and delivered to the user through the IM layer.

This architecture is especially useful for engineering workflows where planning, execution, verification, and memory continuity all matter.

## Repository Structure

- `agent_workspace/`: frontend workspace for contact chat, task interaction, and operational UI
- `agent_orchestrator/`: main orchestration backend for conversation flow, tool calling, task planning, and execution coordination
- `memory_server/`: memory service for summaries, rollups, memory retrieval, and memory administration
- `im_service/`: IM-oriented delivery layer for user/contact messaging and async response transport
- `contact_task_service/`: task platform for task persistence, scheduling, lifecycle transitions, and execution-facing APIs
- `openai-codex-gateway/`: OpenAI-compatible gateway for external clients and SDKs

## Why This Architecture Matters

The system is intentionally decomposed into focused services so each domain can evolve without collapsing into one monolith:
- the workspace can optimize for user experience
- the orchestrator can optimize for model behavior and execution policy
- the memory service can optimize for summarization and recall quality
- the task platform can optimize for lifecycle, scheduling, and observability
- the IM layer can optimize for asynchronous delivery semantics
- the gateway can optimize for compatibility

Together, these layers make it possible to support both conversational collaboration and long-running agent execution in one platform.

## Quick Start

From the repository root:

```bash
./restart_services.sh restart
```

Useful commands:

```bash
./restart_services.sh status
./restart_services.sh stop
```

Default runtime logs are written under `logs/`, including:
- `logs/backend.log`
- `logs/frontend.log`
- `logs/memory_backend.log`
- `logs/memory_frontend.log`

## Additional Documentation

- [中文说明](./README.zh-CN.md)
- [Bilingual overview](./README.md)

Subproject READMEs:
- [agent_workspace English](./agent_workspace/README.en.md)
- [agent_workspace 中文](./agent_workspace/README.zh-CN.md)
- [agent_orchestrator English](./agent_orchestrator/README.en.md)
- [agent_orchestrator 中文](./agent_orchestrator/README.zh-CN.md)
- [im_service English](./im_service/README.en.md)
- [im_service 中文](./im_service/README.zh-CN.md)
- [contact_task_service English](./contact_task_service/README.en.md)
- [contact_task_service 中文](./contact_task_service/README.zh-CN.md)
- [memory_server English](./memory_server/README.en.md)
- [memory_server 中文](./memory_server/README.zh-CN.md)
- [openai-codex-gateway English](./openai-codex-gateway/README.en.md)
- [openai-codex-gateway 中文](./openai-codex-gateway/README.zh-CN.md)

## Planning Archive

Historical plans and implementation notes are stored in the local `docs/plans/` directory and are intentionally excluded from git.

## License

This project is licensed under the [MIT License](./LICENSE).
