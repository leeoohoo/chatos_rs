# chat_app_server_rs (Main Backend)

## Positioning
`chat_app_server_rs` is the primary orchestration backend of Chatos RS.
It handles sessions, messages, tool routing, and model streaming so the frontend can deliver a reliable engineering workflow.

## What It Solves
Common backend pain points in AI systems:
- mixing business logic, model calls, and tool logic in one fragile flow,
- unstable multi-turn context orchestration,
- hard-to-debug runtime behavior when tool/model execution interleaves.

This service centralizes orchestration and protocol handling so the system stays predictable under complex multi-step tasks.

## Core Advantages
1. Orchestration-first design
- Separates chat flow control from memory domain and gateway concerns.

2. Real-time interaction support
- Built to support streaming model responses and interactive tool pipelines.

3. Production-friendly Rust stack
- Axum + Tokio architecture targets performance and operational stability.

4. Easy full-stack integration
- Works directly with the memory service and frontend in local all-in-one startup.

## Tech Stack
- Rust (Axum + Tokio)
- SQLx (SQLite)
- MongoDB client support

## Run (Development)
From this directory:

```bash
cargo run --bin chat_app_server_rs
```

## Build
```bash
cargo build --release
```

## Basic Checks
```bash
cargo check
```

## Full-Stack Startup
From repository root:

```bash
./restart_services.sh restart
```
