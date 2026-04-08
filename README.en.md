# agent stack

## Positioning
`agent stack` is an AI platform for engineering and collaborative workflows.  
It combines conversational interaction, tool execution, long-term memory, and OpenAI-compatible access in one system so AI can run as a reliable ongoing worker, not only a one-shot chatbot.

## What Problems It Solves
Typical issues in engineering-grade chat AI systems:
- Context is trapped in a single session and is hard to carry forward.
- Token cost keeps increasing as history grows.
- Tool integration is fragmented and expensive to maintain.
- External integrations are difficult when protocol expectations differ.

`agent stack` addresses these with a layered architecture:
main chat service + memory service + compatibility gateway.

## Core Advantages
1. Long-term memory by design
- Supports session summaries, rollups, and memory consolidation to preserve facts, decisions, and TODOs across sessions.

2. Controlled context cost
- Uses layered summarization and scheduled jobs to compress context while maintaining continuity.

3. Tool-friendly orchestration
- Built for tool calls and MCP-like workflows, making it practical for real engineering pipelines.

4. Scalable architecture
- Frontend, backend, memory domain, and gateway are decoupled and can scale independently.

5. Strong ecosystem compatibility
- Exposes OpenAI-compatible APIs so existing clients and SDKs can integrate with low migration effort.

## Architecture Layers
- `agent_workspace/`: frontend interaction layer
- `agent_orchestrator/`: main orchestration backend (sessions, messages, tools, streaming)
- `memory_server/`: memory domain (summaries, rollups, memory retrieval, admin console)
- `openai-codex-gateway/`: OpenAI-compatible gateway layer

## Quick Start
Run from repository root:

```bash
./restart_services.sh restart
```

Useful commands:

```bash
./restart_services.sh status
./restart_services.sh stop
```

Default runtime logs:
- `logs/backend.log`
- `logs/frontend.log`
- `logs/memory_backend.log`
- `logs/memory_frontend.log`

## Development Plans Archive
Historical plans/assessments/contracts are centralized at:
- local `docs/plans/` directory (intentionally excluded from git)

## Per-Project READMEs
- [agent_workspace English](./agent_workspace/README.en.md)
- [agent_workspace 中文](./agent_workspace/README.zh-CN.md)
- [agent_orchestrator English](./agent_orchestrator/README.en.md)
- [agent_orchestrator 中文](./agent_orchestrator/README.zh-CN.md)
- [memory_server English](./memory_server/README.en.md)
- [memory_server 中文](./memory_server/README.zh-CN.md)
- [openai-codex-gateway English](./openai-codex-gateway/README.en.md)
- [openai-codex-gateway 中文](./openai-codex-gateway/README.zh-CN.md)

## License
This project is licensed under the [MIT License](./LICENSE).
