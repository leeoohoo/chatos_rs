# Agent Workspace

Agent Workspace is the primary user-facing frontend of Agent Stack.

It provides an IM-style collaboration interface where users can talk to AI contacts, review task proposals, receive asynchronous task results, inspect runtime context, and operate the system through a workspace-oriented experience instead of a raw model console.

Agent Workspace 是 Agent Stack 面向用户的主前端工作空间。

它提供一种偏 IM 的协作体验：用户可以像和联系人聊天一样与 AI 交互，查看任务确认卡片，接收后台任务回传结果，并在统一界面中完成上下文查看、任务交互和系统操作。

## What This Module Does

- Provides the main contact-chat and workspace UI
- Presents task review, confirmation, status, and execution history
- Connects to backend services through HTTP and WebSocket channels
- Surfaces runtime context and system responses in a user-friendly way
- Acts as the main operational entry point for Agent Stack

## Why It Exists

Most AI UIs are optimized for direct streaming output, but real work often needs a different interaction model:
- users need message-oriented collaboration, not raw tool traces
- long-running tasks should continue in the background
- results should return asynchronously after execution completes
- task planning and confirmation should be visible and operable
- multiple services should feel like one coherent product

Agent Workspace exists to provide that product layer.

## Key Experience Goals

- IM-first interaction: the interface feels like talking to a capable teammate
- Operational visibility: users can see task status, review payloads, and important context
- Async-friendly UX: background work is decoupled from immediate message rendering
- Multi-surface coordination: chat, task management, memory-related views, and workspace actions live in one place

## Tech Stack

- React 18
- TypeScript
- Vite
- Zustand

## Local Development

Run in this directory:

```bash
npm install
npm run dev
```

## Build

```bash
npm run build
```

## Common Scripts

- `npm run dev`
- `npm run build`
- `npm run preview`
- `npm run type-check`
- `npm run test`
- `npm run lint`

## Integrated Startup

From the repository root:

```bash
./restart_services.sh restart
```

## More Docs

- [中文说明](./README.zh-CN.md)
- [English README](./README.en.md)
- [Usage Notes](./USAGE.md)
