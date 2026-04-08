# Memory Server

## Overview

Memory Server is the long-term memory domain service of Agent Stack.

It stores message history, summaries, rollups, and memory artifacts, and provides the data pipeline that turns raw history into reusable context for future orchestration.

## What It Is Responsible For

- session and message persistence
- layered summary generation and rollups
- memory-oriented retrieval and context composition
- admin-facing operational visibility over memory quality and jobs

## Why It Matters

Persistent agent systems need more than chat logs.

This service exists so the platform can:
- keep context continuity across time
- control prompt cost as history expands
- preserve key decisions and facts instead of replaying everything
- operate memory strategy as a dedicated domain

## Structure

- `backend/`: Rust service
- `frontend/`: React admin console
- `shared/`: shared contracts and assets

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
