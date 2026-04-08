# agent stack

agent stack is an AI platform for engineering workflows.
It combines conversational collaboration, tool orchestration, long-term memory, and OpenAI-compatible access in one system.

agent stack 是一个面向工程协作场景的 AI 平台，统一了对话协作、工具编排、长期记忆和 OpenAI 兼容接入能力。

## Why This System
- Keep context across sessions with memory and summarization.
- Reduce context cost with layered summaries and scheduled processing.
- Make tool execution observable and operable in chat workflows.
- Stay compatible with existing OpenAI-style clients and SDKs.

## Architecture
- `agent_workspace/`: Frontend interaction layer
- `agent_orchestrator/`: Main orchestration backend
- `memory_server/`: Memory domain service
- `openai-codex-gateway/`: OpenAI-compatible gateway

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

Default logs:
- `logs/backend.log`
- `logs/frontend.log`
- `logs/memory_backend.log`
- `logs/memory_frontend.log`

## Language Docs
- [中文](./README.zh-CN.md)
- [English](./README.en.md)

## Subproject READMEs
- [agent_workspace English](./agent_workspace/README.en.md)
- [agent_workspace 中文](./agent_workspace/README.zh-CN.md)
- [agent_orchestrator English](./agent_orchestrator/README.en.md)
- [agent_orchestrator 中文](./agent_orchestrator/README.zh-CN.md)
- [memory_server English](./memory_server/README.en.md)
- [memory_server 中文](./memory_server/README.zh-CN.md)
- [openai-codex-gateway English](./openai-codex-gateway/README.en.md)
- [openai-codex-gateway 中文](./openai-codex-gateway/README.zh-CN.md)

## Note
Development plan documents are kept under local `docs/plans/` and are intentionally not tracked in git.

## License
This project is licensed under the [MIT License](./LICENSE).
