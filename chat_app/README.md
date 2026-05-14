# chat_app (Frontend)

This directory is the primary React frontend for Chatos RS, not a standalone npm chat component package.

## Positioning

`chat_app` is where users chat with AI, inspect tool execution, manage sessions, and continue engineering workflows across turns.

## What It Includes

- Session-oriented chat UI
- Realtime stream and tool event rendering
- Project explorer, terminal, and related workflow surfaces
- Auth-aware app shell and settings flows
- Frontend integration for backend runtime, MCP, and memory features

## Tech Stack

- React 18
- TypeScript
- Vite
- Zustand
- Vitest

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
# or
make restart
```

## Note

Historical component-library-oriented docs may still exist in older files such as `USAGE.md`. The active source of truth for this repository is the root README plus `README.en.md` / `README.zh-CN.md` in each subproject.
