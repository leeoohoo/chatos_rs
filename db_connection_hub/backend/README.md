# Backend（Rust）

当前后端已实现一个可运行的模块化骨架（`axum + tokio`），重点覆盖：
- 数据源创建/更新/测试
- 连接下 database 统计与列表
- database 下对象统计
- 元数据树懒加载与对象详情
- 手写 SQL 执行与取消

## 目录结构（已落地）

- `src/api`：HTTP 路由与 handler
- `src/domain`：领域模型（datasource/meta/metadata/query）
- `src/service`：业务编排
- `src/repository`：仓储抽象与内存实现
- `src/drivers`：驱动 trait、registry、真实驱动与 mock 驱动
- `src/bootstrap.rs`：依赖装配
- `src/state.rs`：应用状态

## 运行方式

```bash
cd db_connection_hub/backend
cargo run
```

默认监听：`0.0.0.0:8099`

可选环境变量：
- `DB_HUB_HOST`
- `DB_HUB_PORT`
- `RUST_LOG`

## 已实现接口

- `GET /api/v1/health`
- `GET /api/v1/meta/db-types`
- `GET /api/v1/datasources`
- `POST /api/v1/datasources`
- `PUT /api/v1/datasources/{id}`
- `POST /api/v1/datasources/{id}/test`
- `GET /api/v1/datasources/{id}/health`
- `GET /api/v1/datasources/{id}/databases/summary`
- `GET /api/v1/datasources/{id}/databases`
- `GET /api/v1/datasources/{id}/databases/{database}/object-stats`
- `GET /api/v1/metadata/nodes`
- `GET /api/v1/metadata/object-detail`
- `POST /api/v1/queries/execute`
- `POST /api/v1/queries/{id}/cancel`

## 当前状态

- 所有接口已联通，`cargo check` 通过
- 已接入“混合驱动”：
  - PostgreSQL：真实连接与元数据查询
    - object detail 支持 `table/view/materialized_view/sequence` 与 `index/trigger`
  - MySQL：真实连接与元数据查询
    - object detail 支持 `table/view` 与 `index/trigger`
  - SQLite：真实连接与元数据查询
    - object detail 支持 `table/view` 与 `index/trigger`
  - SQL Server：真实连接与元数据查询
    - object detail 支持 `table/view` 与 `index/trigger/procedure/function/sequence/synonym`
  - MongoDB：真实连接与元数据查询（数据库/collection/index）
    - object detail 支持 `collection/view` 与 `index`
  - Oracle：第一阶段真实驱动（网络探测 + 投影式元数据树）
    - schema 下支持 table/view/materialized_view/sequence/procedure/function/synonym/package
    - table 下支持 index/trigger
    - object-stats 返回上述类型统计并标记 `partial=true`
- 下一步补齐 Oracle 真实 SQL 执行与深度元数据查询
