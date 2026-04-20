# DB Connection Hub：实施方案（Rust + React，纯数据库连接阶段）

## 1. 目标与边界

目标：构建一个统一的数据库连接与元数据浏览服务，支持主流数据库的连接、认证、对象树浏览与基础 SQL 执行。

当前阶段明确不做：
- AI 生成 SQL
- 智能诊断/自动修复

## 2. MVP 功能范围

1. 数据源管理
- 创建/更新/删除连接
- 连接测试（网络、认证、权限、版本）
- 连接健康状态（在线/离线/慢）

2. 认证与网络
- 账号密码、TLS/mTLS、Token、文件型认证（按数据库能力）
- 代理与隧道（直连、SSH Tunnel）

3. 元数据浏览
- 连接下 database 数量统计
- database 下 table/view/index/procedure/function 等对象数量统计
- 对象树按层级展开浏览

4. 基础查询
- 手写 SQL 执行（后续可限定只读）
- 分页与超时
- 查询审计

## 3. 架构设计

```text
React Web
  -> API Gateway (Axum)
  -> Application Service
      -> Driver Registry (Trait Object)
          - PostgreSQL Driver
          - MySQL Driver
          - SQLite Driver
          - SQL Server Driver
          - Oracle Driver
          - MongoDB Driver
      -> Metadata Explorer Service
      -> Connection Test Service
      -> Policy Service
  -> Metadata Store (PostgreSQL)
  -> Secret Encryptor (AES-GCM / KMS)
  -> Audit Store
```

## 4. 后端分层（Rust）

- `api`：HTTP 路由、请求校验、统一错误码
- `app`：用例编排（创建连接、测试连接、拉取元数据）
- `domain`：统一模型（DataSource、AuthProfile、TreeNode、ObjectStats）
- `infra`：各数据库驱动实现、凭据加密、持久化

核心 trait 建议：

```rust
#[async_trait::async_trait]
pub trait DatabaseDriver {
    async fn test_connection(&self, req: TestConnectionRequest) -> anyhow::Result<TestConnectionResult>;
    async fn list_databases(&self, req: ListDatabasesRequest) -> anyhow::Result<Vec<DatabaseInfo>>;
    async fn list_children(&self, req: ListChildrenRequest) -> anyhow::Result<Vec<MetadataNode>>;
    async fn get_object_stats(&self, req: ObjectStatsRequest) -> anyhow::Result<ObjectStats>;
    async fn execute(&self, req: ExecuteRequest) -> anyhow::Result<ExecuteResult>;
}
```

## 5. 认证支持策略

详细矩阵见：`docs/authentication-support-matrix.md`

原则：
1. 前端认证选项按数据库动态展示，不让用户填写无效字段
2. 凭据与证书统一加密存储（字段级）
3. 认证与网络解耦（同一认证可叠加直连/SSH/TLS）
4. 对未支持认证类型明确“阶段计划”而不是隐藏

## 6. 元数据浏览模型

详细模型见：`docs/metadata-explorer-design.md`

关键点：
1. 统一节点模型（database/schema/table/view/index...）
2. 每个驱动声明自身 `capabilities`，前端按能力渲染
3. 同层统计与懒加载结合，避免首次加载过慢
4. 不同数据库结构差异通过“映射层”统一，不把差异暴露给 UI

## 7. 前端设计（React）

页面：
- `/connections`：连接列表（显示数据库数量、健康状态）
- `/connections/new`：连接向导（数据库类型 -> 网络 -> 认证 -> 测试）
- `/connections/:id/explorer`：对象浏览树 + 对象统计
- `/workbench`：基础 SQL 执行页面（非 AI）

交互要求：
1. 点击连接卡片，先返回 database 数量与最近测试结果
2. 点击某个 database，显示对象分类统计（table/view/index 等）
3. 点击对象节点，展示详情（列、索引列、约束、DDL 预览）

## 8. 数据模型建议

1. `DataSource`
- id, name, db_type, network, auth_profile, options, tags, status

2. `AuthProfile`
- mode（password/cert/token/integrated/...）
- secret_ref（引用密文，不直存明文）
- tls_config_ref

3. `MetadataNode`
- id, parent_id, node_type, display_name, raw_name, path, has_children, extra

4. `ObjectStats`
- database_count
- table_count, view_count, index_count
- procedure_count, function_count, trigger_count
- sequence_count, synonym_count（按数据库可选）

## 9. 性能与稳定性

1. 连接池：按数据源维度控制 min/max
2. 超时：连接超时 + 语句超时分开配置
3. 缓存：元数据树节点缓存（TTL 30s~120s）
4. 限流：同连接并发查询数限制
5. 降级：统计失败不阻塞树结构展示

## 10. 安全与合规

1. 凭据加密：敏感字段仅密文落库
2. 最小权限：建议连接账号默认只读
3. 审计：记录连接测试与查询操作
4. 脱敏：日志中不打印密码、token、证书内容
5. 权限：RBAC（查看连接/管理连接/执行 SQL 分离）

## 11. 里程碑建议（6 周）

1. 第 1 周：项目骨架 + 统一模型 + PostgreSQL 基础连接
2. 第 2 周：MySQL/SQLite + 连接向导动态表单
3. 第 3 周：对象树 API（database/schema/table/view/index 统计）
4. 第 4 周：SQL Server/Oracle 的连接与元数据抽象接入
5. 第 5 周：MongoDB 浏览支持 + 前端对象详情页
6. 第 6 周：压测、错误码打磨、审计与权限收口

## 12. 主要风险与应对

- 风险：各数据库对象模型差异大
  - 应对：驱动能力声明 + 统一节点抽象 + 前端按能力展示
- 风险：认证方式复杂，配置容易失败
  - 应对：向导分步校验 + 实时连接测试 + 明确错误分类
- 风险：元数据量巨大导致卡顿
  - 应对：懒加载 + 分页 + 节点缓存 + 后台预热
