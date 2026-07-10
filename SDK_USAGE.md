# Memory Engine SDK 使用说明

`memory_engine_sdk` 是 Rust 版客户端，用来调用 Memory Engine 的线程、消息记录、总结、记忆、上下文和后台任务接口。

它默认对应的是“平台直接接管线程和消息数据”的接入模式。接入系统不需要自己再维护另一套独立的会话/消息存储，只需要把线程和消息写入平台即可。

## 1. 引入依赖

```toml
[dependencies]
memory_engine_sdk = { path = "crates/memory_engine_sdk" }
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
- `ingest_thread_records`
- `batch_sync_records`（兼容旧名称）
- `list_thread_records`
- `compose_context`
- `upsert_thread_snapshot`
- `get_latest_thread_snapshot`
- `list_thread_summaries`
- `run_thread_summary`
- `run_thread_repair_summary`
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

### 4.3 写入消息记录

```rust
use memory_engine_sdk::{SdkBatchSyncRecordsRequest, UpsertRecordInput};
use serde_json::json;

let resp = client.ingest_thread_records(
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

### 4.5 触发复盘总结（thread repair）

```rust
let resp = client
    .run_thread_repair_summary("thread_001", "tenant_001")
    .await?;

if resp.accepted && resp.running {
    println!(
        "repair accepted, still running, job_run_id={:?}, pending_source_records={}",
        resp.job_run_id,
        resp.source_record_count
    );
}
```

`run_thread_repair_summary` 现在是“立即接单、后台执行”的语义：

- 接口成功返回时，通常会得到 `accepted=true`、`running=true`
- `job_run_id` 表示这次后台复盘任务对应的 job id
- `generated=false` 不再代表失败，只表示这次 HTTP 返回时任务还没跑完
- 如果当前线程没有待复盘消息，会返回 `accepted=false`、`running=false`
- 如果同一线程已经有一个 `thread_repair` 正在执行，会直接复用现有运行中的 job，并返回它的 `job_run_id`

调用方建议：

- 不要再把 `generated=false` 直接当成“复盘失败”
- 应该把 `accepted=true && running=true` 视为“已成功发起后台复盘”
- 最终状态请通过你自己的状态接口、任务面板或实时事件判断

## 5. 认证模式说明

- `new_direct`：适合业务方直接按 `source_id` 接入。
- `new_system`：适合平台侧或内部系统级调用。

## 6. 常见约定

- `base_url` 建议传 `http://127.0.0.1:7081`
- `timeout` 建议至少 `30s`
- `run_thread_repair_summary` 不需要把 HTTP timeout 调得特别长，因为它会立即返回后台运行状态
- 所有接口都返回 `Result<_, String>`
- 失败时直接读取错误字符串即可

## 7. 本地启动

如果你要先启动服务，再接 SDK：

```bash
./start_all.sh
```

后端默认端口是 `7081`，前端默认端口是 `4178`。
