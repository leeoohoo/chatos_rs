# Contact Task Service

## Overview

Contact Task Service is the task platform of Agent Stack.

It manages task persistence, lifecycle transitions, scheduling, and execution-facing APIs so tasks can be modeled as durable workflow objects rather than temporary chat byproducts.

## What It Is Responsible For

- task persistence
- task status and lifecycle management
- scheduling and execution-facing task APIs
- task-oriented operational UI

## Structure

- `backend/`: Rust service
- `frontend/`: React console

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
