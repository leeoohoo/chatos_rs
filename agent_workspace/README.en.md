# agent_workspace (Frontend)

## Positioning
`agent_workspace` is the primary user-facing interface of the agent stack.
It is where users chat with AI, trigger tools, inspect progress, and continue work across sessions.

## What It Solves
In daily AI usage, frontend issues usually include:
- weak interaction feedback while the model is working,
- poor visibility for tool execution,
- fragmented UX between short chat and long-running tasks.

`agent_workspace` provides a consistent interaction layer so users can run engineering workflows, not just ask one-off questions.

## Core Advantages
1. Workflow-oriented UX
- Designed for iterative work and multi-turn collaboration rather than simple Q&A.

2. Better task continuity
- Works with backend memory/context services so users can resume ongoing work naturally.

3. Fast iteration speed
- React + Vite + TypeScript setup keeps local development and UI iteration efficient.

4. Operationally friendly
- Can be started together with backend services from the repo root for full-stack local runs.

## Tech Stack
- React 18
- TypeScript
- Vite
- Zustand

## Local Development
Run from this directory:

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

## Full-Stack Startup
From repository root:

```bash
./restart_services.sh restart
```
