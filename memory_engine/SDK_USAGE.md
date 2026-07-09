# Memory Engine SDK 使用文档

`memory_engine_sdk` 是 Rust 版客户端，用来调用 Memory Engine 的线程、消息记录、总结、记忆、上下文和后台任务接口。

它默认对应的是“平台直接接管线程和消息数据”的接入模式。接入系统不需要自己再维护另一套独立的会话/消息存储，只需要把线程和消息写入平台即可。

## 1. 引入依赖

```toml
[dependencies]
memory_engine_sdk = { path = "../crates/memory_engine_sdk" }
```

## 2. 初始化

如果你是“业务 source 直接接入”的模式，请使用 `new_direct`：

```rust
use memory_engine_sdk::MemoryEngineClient;
use std::time::Duration;

let client = MemoryEngineClient::new_direct(
    "http://127.0.0.1:7081",
    Duration::from_secs(30),
    "your_source_id",
)?
.with_operator_token("your_operator_token");
```

如果你是“平台直接接管线程和消息数据”的模式，可以使用 `new_platform`：

```rust
let client = MemoryEngineClient::new_platform(
    "http://127.0.0.1:7081",
    Duration::from_secs(30),
)?
.with_operator_token("your_operator_token");
```

注意：

- `new_platform` 适合不绑定单一 `source_id` 的平台级读查询或按需触发的聚合任务
- 需要明确 `source_id` 作用域的 direct 写接口或线程级调度接口，应该改用 `new_direct`
- 如果误用 `new_platform` 调用这类必须绑定 source 的接口，SDK 现在会尽早返回明确错误
- 如果后端配置了 `MEMORY_ENGINE_OPERATOR_TOKEN`，请在 client 初始化后链式调用 `.with_operator_token(...)`

如果你使用系统级账号：

```rust
let client = MemoryEngineClient::new_system(
    "http://127.0.0.1:7081",
    Duration::from_secs(30),
    "your_system_id",
    "your_secret_key",
)?
.with_operator_token("your_operator_token");
```

## 3. 常用接口

- `upsert_thread`
- `get_thread`
- `list_threads`
- `delete_thread`
- `ingest_thread_records`
- `batch_sync_records`（兼容旧名称）
- `list_thread_records`
- `list_thread_records_page`
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
        recent_record_limit: None,
        summary_limit: None,
    }),
}).await?;
```

### 4.5 分页读取线程记录

```rust
use memory_engine_sdk::SdkListThreadRecordsRequest;

let page = client
    .list_thread_records_page(
        "thread_001",
        &SdkListThreadRecordsRequest {
            tenant_id: "tenant_001".to_string(),
            role: None,
            record_type: Some("message".to_string()),
            summary_status: None,
            limit: Some(20),
            offset: Some(0),
            order: Some("asc".to_string()),
        },
    )
    .await?;

println!("total={}", page.total);
println!("page_size={}", page.items.len());
```

兼容说明：

- `list_thread_records_page` 返回后端分页契约：`items + total`
- `list_thread_records` 继续保留，语义不变，只返回 `Vec<EngineRecord>`
- 如果调用方需要分页总数或与管理台接口对齐，优先使用 `list_thread_records_page`

### 4.6 触发复盘总结（thread repair）

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
- `new_platform`：适合平台侧不预绑定单一 `source_id` 的 direct 读查询；若要调用依赖明确 source 作用域的写接口，请改用 `new_direct`。
- `new_system`：适合平台侧或内部系统级调用。
- 三种模式在后端启用 operator 鉴权时，都可以链式调用 `.with_operator_token("...")` 透传管理令牌。

## 6. 常见约定

- `base_url` 建议传 `http://127.0.0.1:7081`
- `timeout` 建议至少 `30s`
- `run_thread_repair_summary` 不需要把 HTTP timeout 调得特别长，因为它会立即返回后台运行状态
- 所有接口都返回 `Result<_, String>`
- 失败时直接读取错误字符串即可

## 7. 启动服务

如果你要先启动服务，再接 SDK：

```bash
docker/deploy.sh up
```

后端默认端口是 `7081`，前端默认端口是 `4178`。
