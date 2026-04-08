# Agent Orchestrator

## Overview

Agent Orchestrator is the core backend orchestration service of Agent Stack.

It coordinates conversation flow, context construction, model invocation, builtin tools, task planning, task execution, and cross-service runtime integration.

## What This Service Is Responsible For

- handling workspace-originated chat requests
- assembling runtime context from messages, summaries, memory, and authorized assets
- routing builtin MCP and tool calls
- driving task review, confirmation, creation, and execution coordination
- connecting the workspace, memory, IM, and task platform into one operating flow

## Why It Matters

This is the service that turns separate capabilities into one working system.

Its role is to prevent:
- prompt logic from being mixed with transport concerns
- tool execution from becoming uncontrolled
- task lifecycle from being hidden inside chat turns
- service boundaries from leaking into user experience

## Tech Stack

- Rust
- Axum
- Tokio
- SQLx with SQLite
- MongoDB client support

## Local Development

In this directory:

```bash
cargo run --bin agent_orchestrator
```

## Build

```bash
cargo build --release
```

## Basic Checks

```bash
cargo check
```

## Integrated Startup

From the repository root:

```bash
./restart_services.sh restart
```
