# Contact Task Service

Contact Task Service is the task platform of Agent Stack.

It manages task persistence, lifecycle transitions, scheduling, execution-facing APIs, and the operational surfaces required to treat tasks as durable system objects rather than temporary chat artifacts.

Contact Task Service 是 Agent Stack 的任务平台。

它负责任务持久化、生命周期状态流转、调度、执行侧接口，以及把任务作为长期系统对象来管理所需的运维与展示能力。

## What This Service Does

- Stores and manages tasks as first-class entities
- Tracks task status, confirmation state, and execution outcome
- Supports scheduling and execution-facing APIs
- Provides a dedicated frontend for task administration and observation

## Structure

- `backend/`: Rust task service
- `frontend/`: React task console

## Backend Quick Start

```bash
cd backend
cargo run --bin contact_task_service
```

## Frontend Quick Start

```bash
cd frontend
npm install
npm run dev
```

## More Docs

- [中文说明](./README.zh-CN.md)
- [English README](./README.en.md)
