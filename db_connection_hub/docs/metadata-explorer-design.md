# 元数据对象树设计（连接 -> database -> table/index/view ...）

## 1. 目标

满足以下核心浏览流程：
1. 点击连接，看到该连接下有多少 `database`（或同等层级）
2. 点击某个 database，看到对象分类统计（table/view/index 等）
3. 下钻到对象详情，查看列、索引、约束、定义

## 2. 统一树模型

统一节点类型（`node_type`）：
- `connection_root`
- `database`
- `schema`
- `collection`
- `table`
- `view`
- `materialized_view`
- `index`
- `sequence`
- `procedure`
- `function`
- `trigger`
- `synonym`
- `package`

统一节点字段：
- `id`：全局唯一节点 ID
- `parent_id`：父节点 ID
- `node_type`
- `display_name`
- `raw_name`
- `path`：如 `db.public.orders`
- `has_children`
- `stats`：可选，当前节点下各类型数量

## 3. 各数据库层级映射

### 3.1 PostgreSQL

层级：
- connection_root
  - database
    - schema
      - table / view / materialized_view / sequence / function
        - index / trigger

点击 database 后统计建议：
- schema_count
- table_count
- view_count
- materialized_view_count
- index_count
- function_count
- sequence_count

### 3.2 MySQL / MariaDB

层级：
- connection_root
  - database（MySQL 中 schema 与 database 同义）
    - table / view / procedure / function / trigger / event
      - index

点击 database 后统计建议：
- table_count
- view_count
- index_count
- procedure_count
- function_count
- trigger_count

### 3.3 SQL Server

层级：
- connection_root
  - database
    - schema
      - table / view / procedure / function / synonym
        - index / trigger

点击 database 后统计建议：
- schema_count
- table_count
- view_count
- index_count
- procedure_count
- function_count
- synonym_count
- trigger_count

### 3.4 Oracle

层级（简化为通用展示）：
- connection_root
  - database_service（可映射为 database）
    - schema（常见为 user）
      - table / view / materialized_view / sequence / procedure / function / package / synonym
        - index / trigger

点击 database 后统计建议：
- schema_count
- table_count
- view_count
- materialized_view_count
- index_count
- procedure_count
- function_count
- package_count
- synonym_count

### 3.5 MongoDB

层级：
- connection_root
  - database
    - collection / view
      - index

点击 database 后统计建议：
- collection_count
- view_count
- index_count

### 3.6 SQLite

层级：
- connection_root（单文件）
  - schema（main/temp）
    - table / view
      - index / trigger

点击连接后统计建议：
- schema_count
- table_count
- view_count
- index_count
- trigger_count

## 4. 能力声明（Capabilities）

每个驱动返回能力声明，前端按能力渲染：

```json
{
  "has_database_level": true,
  "has_schema_level": true,
  "supports_materialized_view": false,
  "supports_synonym": false,
  "supports_package": false,
  "supports_trigger": true
}
```

## 5. 统计策略

1. 首次点击连接：
- 返回 database 列表（分页）
- 同时返回 `database_count`

2. 点击 database：
- 先返回各对象类别数量（summary）
- 用户继续展开时再加载具体对象（lazy load）

3. 大实例优化：
- 统计查询超时时可返回 `partial=true`
- 对对象列表分页（避免一次返回 10w+）
- 缓存统计结果 TTL 30~120 秒

## 6. API 与 UI 交互建议

1. `GET /datasources/{id}/databases/summary`
- 返回总 database 数量 + 活跃/不可见数量

2. `GET /datasources/{id}/databases`
- 返回 database 列表（分页）

3. `GET /datasources/{id}/databases/{db}/object-stats`
- 返回 table/view/index 等数量

4. `GET /metadata/nodes?parent_id=...`
- 懒加载某节点的子节点列表

5. `GET /metadata/object-detail?node_id=...`
- 返回对象字段、索引列、约束、定义语句

## 7. 权限与可见性

- 只展示当前账号有权限访问的对象
- 统计值默认基于“可见对象”
- 返回 `visibility_scope=limited/full`，前端标注“仅显示有权限对象”

## 8. 异常处理

- 统计失败：返回空统计 + 错误原因，不阻断树加载
- 局部节点失败：节点上标记 warning，可单节点重试
- 权限不足：节点可见但不可展开，附错误提示

## 9. 元数据来源与最小权限建议

### PostgreSQL

元数据来源：
- `pg_database`
- `pg_namespace`
- `pg_class`
- `pg_indexes`
- `pg_proc`
- `pg_trigger`

最小权限建议：
- 至少可访问系统 catalog
- 对业务 schema 需要 `USAGE`
- 对表详情建议有 `SELECT` 或 metadata 可见权限

### MySQL / MariaDB

元数据来源：
- `information_schema.SCHEMATA`
- `information_schema.TABLES`
- `information_schema.VIEWS`
- `information_schema.STATISTICS`
- `information_schema.ROUTINES`
- `information_schema.TRIGGERS`

最小权限建议：
- 至少可读 `information_schema`
- 对业务库建议授予最小只读权限

### SQL Server

元数据来源：
- `sys.databases`
- `sys.schemas`
- `sys.tables`
- `sys.views`
- `sys.indexes`
- `sys.procedures`
- `sys.triggers`

最小权限建议：
- 可见数据库目录
- 数据库内具备 metadata 可见权限（例如 `VIEW DEFINITION`）

### Oracle

元数据来源：
- `ALL_USERS`
- `ALL_TABLES`
- `ALL_VIEWS`
- `ALL_INDEXES`
- `ALL_OBJECTS`
- `ALL_TRIGGERS`

最小权限建议：
- 使用 `ALL_*` 视图避免过高权限
- 如需全局视图再评估 `DBA_*` 权限

### MongoDB

元数据来源：
- `listDatabaseNames`
- `listCollections`
- `listIndexes`

最小权限建议：
- 具备目标数据库的 `read` 权限
- 如需列出全部 database，需要额外 catalog 权限

### SQLite

元数据来源：
- `sqlite_master`
- `pragma table_info(...)`
- `pragma index_list(...)`
- `pragma index_info(...)`

最小权限建议：
- 文件可读（只读模式可选）

## 10. 统计与节点加载 SLA 建议

- 单次节点展开目标：`P95 < 800ms`
- database 对象统计目标：`P95 < 1500ms`
- 超时策略：`metadata_timeout_ms` 默认 3000ms，可配置
- 失败重试：指数退避，最多 2 次

## 11. 前端展示一致性规则

1. 不存在的对象类型不展示（例如 SQLite 无 procedure）
2. 统计值未知时显示 `-`，并可点击重试
3. 同一层节点按固定排序：
- schema/database 节点按名称
- 对象分类节点按类型优先级（table > view > index > procedure > function ...）
4. 每个节点都要有“可复制路径”（如 `orders.public.orders`）
