# Agent Stack

Agent Stack is an AI agent infrastructure project for engineering work, persistent collaboration, and asynchronous task execution.

It is designed to move AI from "single-turn chat" to "continuously operating teammate": an agent can talk with users, remember important context, plan tasks, call tools, execute work asynchronously, and deliver results back through a unified interaction layer.

Agent Stack 是一个面向工程协作、长期上下文与异步任务执行的 AI Agent 基础设施项目。

它的目标不是做一个一次性回答问题的聊天机器人，而是让 AI 具备“持续工作”的能力：可以与用户对话、保留关键记忆、拆解并创建任务、调用工具、异步执行，再把结果回传给用户。

## What This Project Is For

This project is built for teams that want AI to participate in real workflows instead of only generating text.

Typical use cases:
- Collaborating with AI contacts in an IM-style interface
- Turning natural-language requests into structured executable tasks
- Running tasks with tool access such as read, write, terminal, remote, and UI prompting
- Preserving knowledge, decisions, and user preferences across sessions
- Exposing the system through OpenAI-compatible APIs for external integrations

## What Problems It Solves

Most chat-based AI systems break down when they are asked to operate like real workers:
- Context is trapped inside one session and becomes hard to carry across time
- Prompt cost grows quickly as history expands
- Tool orchestration is fragile and difficult to observe
- Task creation, confirmation, execution, and result delivery are often split across different systems
- External systems need compatibility layers before they can integrate

Agent Stack addresses these problems with a service-oriented architecture that separates interaction, orchestration, memory, IM delivery, and compatibility access while keeping them connected through explicit runtime contracts.

## Core Capabilities

- Persistent context: layered summaries, rollups, and memory consolidation keep important facts available without replaying full history
- IM-first collaboration: users interact with AI contacts through a conversation model that feels like messaging, while execution happens behind the scenes
- Structured task planning: AI can create reviewable task graphs with dependencies, confirmation flow, execution state, and result reporting
- Tool-based execution: tasks can run with controlled builtin MCP/tool capabilities and scoped runtime context
- Async orchestration: user interaction and background execution are decoupled, so long-running work can continue after a message round ends
- OpenAI compatibility: a gateway layer allows external clients and SDKs to integrate using familiar API conventions

## How The System Works

At a high level, Agent Stack turns user intent into durable, executable work:

1. A user sends a message to an AI contact through the workspace UI.
2. The orchestration layer builds context from conversation history, summaries, memory, and authorized runtime resources.
3. The AI either replies directly or creates one or more tasks that can be reviewed and confirmed.
4. Confirmed tasks are executed asynchronously with the required tools, assets, and scoped runtime information.
5. Execution results, summaries, and follow-up context flow back into memory and are delivered to the user through the IM layer.

This makes the system suitable for engineering scenarios where planning, execution, verification, and recall all matter.

## Repository Architecture

- `agent_workspace/`: frontend workspace for IM-style contact collaboration, task interaction, and operational views
- `agent_orchestrator/`: main backend for conversation orchestration, tool execution, task planning, task execution, and runtime coordination
- `memory_server/`: memory domain service for summaries, rollups, memory retrieval, and memory-oriented administration
- `im_service/`: IM-facing service that manages user/contact messaging delivery and asynchronous conversation transport
- `contact_task_service/`: task platform service for task persistence, scheduling, task lifecycle, and execution-facing task APIs
- `openai-codex-gateway/`: compatibility gateway for OpenAI-style clients and SDKs

## Why The Architecture Matters

The project is intentionally split into focused services so each concern can evolve independently:
- interaction can feel like messaging instead of raw model streaming
- orchestration can focus on prompts, tools, and execution policy
- memory can evolve as its own domain with dedicated summarization strategy
- task lifecycle can be managed explicitly instead of being hidden inside chat turns
- external integrations can reuse standard API shapes without coupling to internal implementation

This separation is what allows the platform to support both interactive chat experiences and long-running background agent workflows.

## Quick Start

Run from the repository root:

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

## Documentation

- [中文说明](./README.zh-CN.md)
- [English README](./README.en.md)

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

## Planning Notes

Historical design and implementation plans are stored in the local `docs/plans/` directory and are intentionally not tracked in git.

## License

This project is licensed under the [MIT License](./LICENSE).
