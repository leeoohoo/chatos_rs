# Agent artifact TTL 与持久删除 outbox

- 时间：2026-09-20 11:31:58 CST（Asia/Shanghai）
- 本轮目标：实现方案要求的 staged artifact TTL 清理和不阻塞请求的可重试远端删除 outbox。
- 起始提交：`c3cc60a1642dd8ebdbe1beb9f50c419fce74d29d`
- 代码提交：`e0f038f620d2b869a02f6f0b42a0d54f5685d75e`

## 实际改动

1. 新增 PostgreSQL 迁移 `0004_agent_artifact_deletion_outbox.sql`，增加 `deleting` 状态和持久删除队列。
2. 单 artifact 删除改为先验证账户归属再持久入队并返回 `202 Accepted`，不再由请求同步等待 MinIO 删除。
3. 新增账户级 `DELETE /api/agent-artifacts`，把当前账户全部 artifact 原子标记并入队。
4. 后台 reconciler 每轮先把超过 TTL 的 `staged` artifact 有界入队，再使用 `FOR UPDATE SKIP LOCKED` claim 删除任务。
5. 对象删除成功后才删除 PostgreSQL 元数据；失败记录截断错误，并按 5 秒至 1 小时有界指数退避重试。
6. staged TTL 与 reconcile 周期通过运行时环境变量配置，默认分别为 24 小时与 60 秒。

## 涉及文件

- `chatos/backend/migrations/postgres/0004_agent_artifact_deletion_outbox.sql`
- `chatos/backend/src/api/agent_artifacts.rs`
- `chatos/backend/src/repositories/agent_artifacts.rs`
- `chatos/backend/src/services/agent_artifact_maintenance.rs`
- `chatos/backend/src/services/mod.rs`
- `chatos/backend/src/modules/app_startup.rs`
- `docker/compose.yml`

## 业务不变量

- artifact 的账户鉴权、对象路径隔离和内容读取权限不变。
- 删除请求不暴露 bucket、object key 或授权信息。
- 对象存储离线不会丢失删除意图，也不会阻塞本地 Agent 消息事务。
- worker 使用有界批次和 claim 租约，支持多实例并发与崩溃恢复。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo test -p chat_app_server_rs agent_artifact`：5 项通过。
- `cargo test -p chat_app_server_rs`：442 项通过、0 失败、1 项 PostgreSQL 环境测试按设计忽略；额外 binary 与公共 facade 测试均通过。

## 并行改动与剩余风险

macOS 工作区原有 5 个并发修改保持未提交。本地 Agent 消息目前没有用户可调用的删除入口，因此本轮提供单 artifact 与账户级入队能力；后续消息删除功能可逐附件调用同一持久 outbox。实际 PostgreSQL 迁移执行仍需部署环境数据库门禁验证。

## 下一步

补齐 artifact 账户级发现/跨设备恢复协议，以及 M4 连续拆分消息观测和四个发送工具的文档附件执行测试矩阵。
