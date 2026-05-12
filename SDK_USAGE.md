# Memory Engine SDK 使用文档

`memory_engine_sdk` 是 Rust 版客户端，用来调用 Memory Engine 的线程、记录、总结、上下文和后台任务接口。

## 1. 引入依赖

```toml
[dependencies]
memory_engine_sdk = { path = "../memory_engine/sdk" }
```

## 2. 初始化

```rust
use memory_engine_sdk::MemoryEngineClient;
use std::time::Duration;

let client = MemoryEngineClient::new_direct(
    "http://127.0.0.1:7081",
    Duration::from_secs(30),
    "your_source_id",
)?;
```

如果你使用系统级账号：

```rust
let client = MemoryEngineClient::new_system(
    "http://127.0.0.1:7081",
    Duration::from_secs(30),
    "your_system_id",
    "your_secret_key",
)?;
```

## 3. 常用接口

- `upsert_thread`
- `get_thread`
- `list_threads`
- `batch_sync_records`
- `list_thread_records`
- `compose_context`
- `upsert_thread_snapshot`
- `get_latest_thread_snapshot`
- `list_thread_summaries`
- `run_thread_summary`
- `run_pending_summaries_once`
- `run_pending_rollups_once`
- `query_subject_memories`
- `upsert_subject_memory_scope`

## 4. 使用示例

### 4.1 创建或更新线程

```rust
use memory_engine_sdk::SdkUpsertThreadRequest;

let thread = client.upsert_thread(
    "thread_001",
    &SdkUpsertThreadRequest {
        tenant_id: "tenant_001".to_string(),
        subject_id: "subject_001".to_string(),
        thread_type: "chat".to_string(),
        external_thread_id: None,
        title: Some("测试线程".to_string()),
        labels: Some(vec!["demo".to_string()]),
        metadata: None,
        status: Some("active".to_string()),
        created_at: None,
        updated_at: None,
        archived_at: None,
    },
).await?;
```

### 4.2 查询线程

```rust
let thread = client.get_thread("thread_001", Some("tenant_001")).await?;
```

### 4.3 同步记录

```rust
use memory_engine_sdk::{SdkBatchSyncRecordsRequest, UpsertRecordInput};
use serde_json::json;

let resp = client.batch_sync_records(
    "thread_001",
    &SdkBatchSyncRecordsRequest {
        tenant_id: "tenant_001".to_string(),
        records: vec![UpsertRecordInput {
            id: "record_001".to_string(),
            external_record_id: None,
            role: "user".to_string(),
            record_type: "message".to_string(),
            content: "hello".to_string(),
            structured_payload: Some(json!({"lang": "zh"})),
            metadata: None,
            summary_status: None,
            summary_id: None,
            summarized_at: None,
            created_at: "2026-05-11T00:00:00Z".to_string(),
        }],
    },
).await?;
```

### 4.4 组装上下文

```rust
use memory_engine_sdk::{ComposeContextPolicy, SdkComposeContextRequest};

let ctx = client.compose_context(&SdkComposeContextRequest {
    tenant_id: "tenant_001".to_string(),
    subject_id: Some("subject_001".to_string()),
    related_subject_ids: None,
    thread_id: "thread_001".to_string(),
    policy: Some(ComposeContextPolicy {
        include_recent_records: Some(true),
        include_thread_summary: Some(true),
        include_subject_memory: Some(true),
        recent_record_limit: Some(20),
        summary_limit: Some(5),
    }),
}).await?;
```

## 5. 认证模式说明

- `new_direct`：适合业务方直接按 `source_id` 接入。
- `new_system`：适合平台侧或内部系统级调用。

## 6. 常见约定

- `base_url` 建议传 `http://127.0.0.1:7081`
- `timeout` 建议至少 `30s`
- 所有接口都返回 `Result<_, String>`
- 失败时直接读取错误字符串即可

## 7. 本地启动

如果你要先启动服务，再接 SDK：

```bash
./start_all.sh
```

后端默认端口是 `7081`，前端默认端口是 `4178`。
