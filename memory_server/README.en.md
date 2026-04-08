# memory_server

## Positioning
`memory_server` is the memory domain of agent stack.
It manages long-horizon context through summary generation, rollups, memory retrieval, and operational tools.

## What It Solves
Without a dedicated memory layer, AI systems often face:
- exploding token cost from raw history replay,
- weak cross-session continuity,
- duplicated or conflicting summaries in scheduled jobs,
- low visibility into memory quality and operations.

`memory_server` addresses this with structured memory pipelines, scheduled consolidation, and admin capabilities.

## Core Advantages
1. Layered memory lifecycle
- Supports session summaries, rollups, and recall-oriented memory extraction.

2. Better cost/quality balance
- Reduces prompt bloat while preserving key decisions, facts, and TODOs.

3. Job safety and consistency
- Designed for scheduled pipelines with lock/idempotency patterns to reduce duplicate processing.

4. Operational tooling included
- Ships with an admin frontend for memory inspection and maintenance.

## Structure
- `backend/`: Rust memory service
- `frontend/`: React admin console

## Backend Quick Start
```bash
cd backend
cp .env.example .env
cargo run --bin memory_server
```

Default backend URL:
- `http://localhost:7080`

Common Mongo envs:
- `MEMORY_SERVER_MONGODB_URI`
- `MEMORY_SERVER_MONGODB_DATABASE`

## Frontend Quick Start
```bash
cd frontend
npm install
npm run dev
```

Default frontend URL:
- `http://localhost:5176`

## Full-Stack Startup
From repository root:

```bash
./restart_services.sh restart
```
