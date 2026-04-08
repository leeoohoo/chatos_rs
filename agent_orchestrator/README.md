# Agent Orchestrator

Agent Orchestrator is the core backend orchestration service of Agent Stack.

It is responsible for conversation flow, context assembly, model invocation, builtin tool routing, task planning, task execution coordination, and service-to-service integration across the platform.

Agent Orchestrator 是 Agent Stack 的核心后端编排服务。

它负责对话流转、上下文组装、模型调用、内置工具路由、任务规划、任务执行协调，以及整个平台内多个服务之间的主业务编排。

## What This Service Does

- Receives and processes workspace-side chat requests
- Builds model context from messages, summaries, memory, and runtime assets
- Exposes builtin MCP/tool capabilities to the model
- Creates, reviews, confirms, and coordinates task execution workflows
- Connects workspace, memory, IM, task platform, and gateway-facing flows

## Why It Is The Core Of The System

Without a dedicated orchestration layer, the platform would quickly collapse into tightly coupled prompt logic, tool logic, and transport logic.

This service exists to keep those responsibilities explicit:
- model behavior is coordinated instead of scattered
- tool execution is managed through runtime policy
- task lifecycle is treated as a first-class concept
- service boundaries remain clean while user experience stays continuous

## Tech Stack

- Rust
- Axum
- Tokio
- SQLx with SQLite
- MongoDB client support

## Local Development

Run in this directory:

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

## More Docs

- [中文说明](./README.zh-CN.md)
- [English README](./README.en.md)
