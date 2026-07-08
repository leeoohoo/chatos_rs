# chatos/backend

Main orchestration backend for Chatos RS. It handles sessions, messages, model streaming, tool routing, and integrations with User Service, Task Runner, Project Management, Local Connector Service, and Memory Engine.

## Docker Stack

From the repository root:

```bash
docker/deploy.sh up
```

Default backend URL: `http://localhost:3997`

## Backend-Only Development

```bash
cargo run --bin chat_app_server_rs
```

## Checks

```bash
cargo fmt --check
cargo check -p chat_app_server_rs
cargo test -p chat_app_server_rs
```
