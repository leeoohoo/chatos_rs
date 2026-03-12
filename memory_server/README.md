# memory_server

`memory_server` is a standalone memory domain service for:
1. Session/message/summaries storage and querying.
2. L0 summary job (messages -> summary).
3. Rollup summary job (summary -> higher-level summary).
4. Context compose API for upstream AI request assembly.
5. Separate React admin console.

## Structure

- `backend/`: Rust server (Axum + MongoDB + workers)
- `frontend/`: React console (Vite)
- `memory_server_architecture_plan.md`: full architecture/design doc

## Backend

### Quick start

```bash
cd backend
cp .env.example .env
cargo run --bin memory_server
```

Server default address: `http://localhost:7080`

MongoDB env:

```bash
MEMORY_SERVER_MONGODB_URI=mongodb://admin:admin@127.0.0.1:27018/admin
MEMORY_SERVER_MONGODB_DATABASE=memory_server
```

Health endpoint:

```bash
curl http://localhost:7080/health
```

## Frontend

### Quick start

```bash
cd frontend
npm install
npm run dev
```

Frontend default address: `http://localhost:5176`

Configure API base by env:

```bash
VITE_MEMORY_API_BASE=http://localhost:7080/api/memory/v1
```

## One-command Start

Run backend + frontend together:

```bash
./start-dev.sh
```

The script behavior is the same as the main project restart style:
1. Kill existing backend/frontend processes (by pid file + occupied ports).
2. Start both services in background (`nohup`).
3. Print pid/log/api URLs.

Commands:

```bash
./start-dev.sh                # default = restart
./start-dev.sh restart
./start-dev.sh stop
./start-dev.sh status
```

Default runtime/log path:

```text
/tmp/memory_server_dev/backend.log
/tmp/memory_server_dev/frontend.log
```

## SQLite -> Mongo Migration

One-time migration command:

```bash
cd backend
cargo run --bin migrate_sqlite_to_mongo -- \
  --sqlite data/memory_server.db \
  --mongo-uri mongodb://admin:admin@127.0.0.1:27018/admin \
  --mongo-db memory_server
```

If you want to clear target collections first:

```bash
cargo run --bin migrate_sqlite_to_mongo -- --drop-target
```

## Key APIs

- `POST /api/memory/v1/auth/login`
- `GET /api/memory/v1/auth/me`
- `POST /api/memory/v1/sessions`
- `POST /api/memory/v1/sessions/:session_id/messages`
- `GET /api/memory/v1/sessions/:session_id/summaries`
- `POST /api/memory/v1/context/compose`
- `PUT /api/memory/v1/configs/summary-job`
- `PUT /api/memory/v1/configs/summary-rollup-job`
- `POST /api/memory/v1/jobs/summary/run-once`
- `POST /api/memory/v1/jobs/summary-rollup/run-once`

## Notes

1. Worker is enabled by default and scans active users automatically.
2. AI call is implemented inside backend with an OpenAI-compatible endpoint strategy.
3. Summary jobs can use per-user model config, or global fallback env (`MEMORY_SERVER_OPENAI_*` / `OPENAI_*`).
4. `MEMORY_SERVER_ALLOW_LOCAL_SUMMARY_FALLBACK` defaults to `false`: missing key/model will fail job and record error instead of silent local fallback.
5. Default admin account is auto-created on startup: `admin / admin`.
6. Auth rules:
   - Admin can view all users' sessions.
   - Normal users are scoped to their own sessions/messages/summaries.
   - Normal user's job config falls back to admin config when user config is missing.
