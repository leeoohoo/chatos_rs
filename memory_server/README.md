# Memory Server

Memory Server is the long-term memory domain service of Agent Stack.

It is responsible for storing messages, summaries, rollups, and memory artifacts, and for turning raw conversational history into reusable context that can support future model calls at lower cost and higher continuity.

Memory Server 是 Agent Stack 的长期记忆域服务。

它负责保存消息、总结、滚动总结和记忆数据，并把原始对话历史沉淀成可复用的上下文，以更低成本、更高连续性支撑后续模型调用。

## What This Service Does

- Stores sessions, messages, summaries, and memory artifacts
- Runs summary and rollup pipelines
- Produces context payloads for upstream orchestration
- Provides an admin-oriented frontend for operating memory data

## Why It Exists

Long-running agent systems cannot rely on raw history replay forever.

This service exists to solve the memory problem structurally:
- reduce prompt cost as history grows
- preserve important facts, decisions, risks, and TODOs
- support multi-layer summarization rather than one flat summary
- make memory quality observable and operable

## Structure

- `backend/`: Rust memory service
- `frontend/`: React admin console
- `shared/`: shared contracts and cross-layer assets

## Backend Quick Start

```bash
cd backend
cp .env.example .env
cargo run --bin memory_server
```

Default backend address:
- `http://localhost:7080`

## Frontend Quick Start

```bash
cd frontend
npm install
npm run dev
```

Default frontend address:
- `http://localhost:5176`

## Integrated Startup

From the repository root:

```bash
./restart_services.sh restart
```

## More Docs

- [中文说明](./README.zh-CN.md)
- [English README](./README.en.md)
