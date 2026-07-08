# chatos/backend

Chatos RS 主编排后端，负责会话、消息、模型流式输出、工具路由，以及 User Service、Task Runner、Project Management、Local Connector Service、Memory Engine 的集成。

## Docker 栈启动

在仓库根目录执行：

```bash
docker/deploy.sh up
```

默认后端地址：`http://localhost:3997`

## 只开发后端

```bash
cargo run --bin chat_app_server_rs
```

## 检查

```bash
cargo fmt --check
cargo check -p chat_app_server_rs
cargo test -p chat_app_server_rs
```
