# Agent Workspace

## Overview

Agent Workspace is the primary frontend of Agent Stack.

It is designed as a workspace-oriented UI for collaborating with AI contacts, reviewing task plans, receiving asynchronous task results, and operating the broader agent system through a messaging-first experience.

## What This Module Is Responsible For

- Contact-based chat and workspace interaction
- Task review, confirmation, and execution visibility
- WebSocket and HTTP integration with backend services
- User-facing presentation of runtime context and system feedback
- A unified operational surface for the platform

## Why This Frontend Matters

This frontend is not just a chat window.

It exists to support a workflow where:
- users message AI contacts as if they were teammates
- task creation can be reviewed before execution
- long-running work continues in the background
- completed work is delivered back asynchronously
- multiple backend capabilities feel like one cohesive product

## Tech Stack

- React 18
- TypeScript
- Vite
- Zustand

## Local Development

From this directory:

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
- [Usage Notes](./USAGE.md)
