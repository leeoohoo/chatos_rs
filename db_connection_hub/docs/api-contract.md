# API Contract（草案，纯数据库连接工具）

## 1. 查询支持能力（用于动态表单）

`GET /api/v1/meta/db-types`

用途：前端在“新建连接”页面动态显示可选认证方式、TLS 字段、对象树能力。

Response:

```json
{
  "items": [
    {
      "db_type": "postgres",
      "label": "PostgreSQL",
      "auth_modes": ["password", "tls_client_cert", "token", "integrated"],
      "network_modes": ["direct", "ssh_tunnel", "proxy"],
      "capabilities": {
        "has_database_level": true,
        "has_schema_level": true,
        "supports_materialized_view": true,
        "supports_trigger": true
      }
    },
    {
      "db_type": "sqlite",
      "label": "SQLite",
      "auth_modes": ["no_auth", "file_key"],
      "network_modes": ["direct"],
      "capabilities": {
        "has_database_level": false,
        "has_schema_level": true,
        "supports_trigger": true
      }
    }
  ]
}
```

## 2. 创建数据源

`POST /api/v1/datasources`

Request（password 模式示例）：

```json
{
  "name": "orders-prod",
  "db_type": "postgres",
  "network": {
    "mode": "direct",
    "host": "10.0.1.25",
    "port": 5432,
    "database": "orders"
  },
  "auth": {
    "mode": "password",
    "username": "readonly",
    "password": "***"
  },
  "tls": {
    "enabled": true,
    "ssl_mode": "verify_full",
    "ca_cert": "-----BEGIN CERTIFICATE-----...",
    "client_cert": null,
    "client_key": null
  },
  "options": {
    "connect_timeout_ms": 5000,
    "statement_timeout_ms": 15000,
    "pool_min": 1,
    "pool_max": 20
  }
}
```

Request（证书模式示例）：

```json
{
  "name": "mongo-x509-prod",
  "db_type": "mongodb",
  "network": {
    "mode": "direct",
    "host": "10.0.2.18",
    "port": 27017,
    "database": "admin"
  },
  "auth": {
    "mode": "tls_client_cert",
    "username": "CN=svc_reader,OU=DBA,O=Example,L=SH,C=CN",
    "client_cert": "-----BEGIN CERTIFICATE-----...",
    "client_key": "-----BEGIN PRIVATE KEY-----..."
  },
  "tls": {
    "enabled": true,
    "ssl_mode": "verify_ca",
    "ca_cert": "-----BEGIN CERTIFICATE-----..."
  }
}
```

Response:

```json
{
  "id": "ds_9f3d7",
  "status": "created"
}
```

## 3. 更新数据源

`PUT /api/v1/datasources/{id}`

用途：修改网络参数、认证信息、超时与连接池设置。支持“凭据轮换不改 ID”。

## 4. 测试连接

`POST /api/v1/datasources/{id}/test`

Response:

```json
{
  "ok": true,
  "latency_ms": 37,
  "server_version": "PostgreSQL 16.1",
  "auth_mode": "password",
  "checks": [
    {"stage": "network", "ok": true},
    {"stage": "tls", "ok": true},
    {"stage": "auth", "ok": true},
    {"stage": "metadata_permission", "ok": true}
  ]
}
```

错误示例：

```json
{
  "ok": false,
  "error_code": "CONN_AUTH_FAILED",
  "message": "authentication failed for user readonly",
  "stage": "auth"
}
```

## 5. 连接健康状态

`GET /api/v1/datasources/{id}/health`

Response:

```json
{
  "status": "online",
  "last_test_at": "2026-04-15T17:40:00Z",
  "last_latency_ms": 42,
  "failed_count_1h": 0
}
```

## 6. 连接下 database 统计

`GET /api/v1/datasources/{id}/databases/summary`

Response:

```json
{
  "database_count": 12,
  "visible_database_count": 10,
  "visibility_scope": "limited"
}
```

## 7. 列出 database

`GET /api/v1/datasources/{id}/databases?page=1&page_size=50&keyword=order`

Response:

```json
{
  "items": [
    {"name": "orders", "owner": "dba", "size_bytes": 2019235840},
    {"name": "orders_archive", "owner": "dba", "size_bytes": 893920384}
  ],
  "page": 1,
  "page_size": 50,
  "total": 2
}
```

## 8. 获取某个 database 的对象统计

`GET /api/v1/datasources/{id}/databases/{database}/object-stats`

Response（PostgreSQL 示例）：

```json
{
  "database": "orders",
  "schema_count": 4,
  "table_count": 132,
  "view_count": 28,
  "materialized_view_count": 5,
  "index_count": 436,
  "function_count": 23,
  "sequence_count": 41,
  "trigger_count": 17,
  "partial": false
}
```

Response（MongoDB 示例）：

```json
{
  "database": "orders",
  "collection_count": 68,
  "view_count": 4,
  "index_count": 190,
  "partial": false
}
```

## 9. 懒加载对象树节点

`GET /api/v1/metadata/nodes?datasource_id=ds_9f3d7&parent_id=node_001&page=1&page_size=100`

Response:

```json
{
  "items": [
    {
      "id": "node_101",
      "parent_id": "node_001",
      "node_type": "schema",
      "display_name": "public",
      "path": "orders.public",
      "has_children": true
    },
    {
      "id": "node_102",
      "parent_id": "node_001",
      "node_type": "schema",
      "display_name": "analytics",
      "path": "orders.analytics",
      "has_children": true
    }
  ],
  "page": 1,
  "page_size": 100,
  "total": 2
}
```

## 10. 查看对象详情

`GET /api/v1/metadata/object-detail?datasource_id=ds_9f3d7&node_id=node_tbl_orders`

Response:

```json
{
  "node_id": "node_tbl_orders",
  "node_type": "table",
  "name": "orders",
  "columns": [
    {"name": "id", "data_type": "bigint", "nullable": false},
    {"name": "amount", "data_type": "numeric(12,2)", "nullable": false}
  ],
  "indexes": [
    {"name": "idx_orders_created_at", "columns": ["created_at"], "is_unique": false}
  ],
  "constraints": [
    {"name": "pk_orders", "type": "PRIMARY KEY", "columns": ["id"]}
  ],
  "ddl": "CREATE TABLE orders (...)"
}
```

## 11. 执行 SQL（手写）

`POST /api/v1/queries/execute`

Request:

```json
{
  "datasource_id": "ds_9f3d7",
  "database": "orders",
  "sql": "select * from orders limit 100",
  "timeout_ms": 10000,
  "max_rows": 1000
}
```

Response:

```json
{
  "query_id": "q_2291",
  "columns": [
    {"name": "id", "type": "bigint"},
    {"name": "amount", "type": "decimal"}
  ],
  "rows": [
    [1, 88.50],
    [2, 19.00]
  ],
  "row_count": 2,
  "elapsed_ms": 24
}
```

## 12. 取消查询

`POST /api/v1/queries/{id}/cancel`

Response:

```json
{
  "query_id": "q_2291",
  "status": "cancelled"
}
```

## 13. 审计查询

`GET /api/v1/audits?datasource_id=ds_9f3d7&action=query_execute&limit=50`

返回字段建议包含：
- `operator`
- `action`（test_connection / query_execute / metadata_read）
- `datasource_id`
- `database`
- `elapsed_ms`
- `status`
- `created_at`
