# Memory Engine SDK 使用文档

`memory_engine_sdk` 是 Memory Engine 的 Rust 客户端。当前契约只有一条生产路径：调用方写入带项目分区和 Runtime Context Assertion 的原子 Source Unit，Runtime 使用冻结模型能力版本计算预算，再调用 compose。

## 1. 引入和初始化

```toml
[dependencies]
memory_engine_sdk = { path = "../crates/memory_engine_sdk" }
```

绑定单一 `source_id` 的调用方使用 direct client：

```rust
use memory_engine_sdk::MemoryEngineClient;
use std::time::Duration;

let client = MemoryEngineClient::new_direct(
    "http://127.0.0.1:7081",
    Duration::from_secs(30),
    "source_001",
)?
.with_internal_service_auth("local-agent-host", "caller-specific-secret");
```

跨 source 的平台服务使用 `new_platform`；系统密钥调用方使用 `new_system`。服务间请求必须配置 `.with_internal_service_auth(caller, secret)`，用户请求使用 `.with_bearer_token(...)`。

## 2. Runtime Context Assertion

写入、compose、总结和删除操作都必须携带短期 Runtime Context Assertion。Assertion 冻结并绑定：

- principal、tenant、source、canonical Agent subject；
- thread 和 `project_partition_key`；
- allowed operation；
- runtime identity revision、签发时间、过期时间和唯一 `jti`。

```rust
use memory_engine_sdk::RuntimeContextAssertionIssuer;

let issuer = RuntimeContextAssertionIssuer::new(
    "runtime-context-signing-secret",
    "local-agent-host-2026-11",
    "local-agent-host",
    "runtime-identity-v3",
    60,
)?;

let assertion = issuer.issue(
    "user_001",
    "tenant_001",
    "source_001",
    "agent:assistant_001",
    "thread_001",
    "project:project_001",
    &["record_ingest", "context_compose", "summary_run"],
)?;
```

服务端会严格校验 scope、operation、签名、有效期和 replay；校验失败时请求直接失败。

## 3. 写入线程和 Source Unit

```rust
use memory_engine_sdk::{SdkBatchSyncRecordsRequest, SdkUpsertThreadRequest, UpsertRecordInput};
use serde_json::json;

client
    .upsert_thread(
        "thread_001",
        &SdkUpsertThreadRequest {
            tenant_id: "tenant_001".to_string(),
            subject_id: "agent:assistant_001".to_string(),
            thread_type: "chat".to_string(),
            external_thread_id: None,
            title: Some("Memory reliability implementation".to_string()),
            labels: None,
            metadata: None,
            status: Some("active".to_string()),
            created_at: None,
            updated_at: None,
            archived_at: None,
        },
    )
    .await?;

let response = client
    .ingest_thread_records(
        "thread_001",
        &SdkBatchSyncRecordsRequest {
            tenant_id: "tenant_001".to_string(),
            project_partition_key: "project:project_001".to_string(),
            runtime_context_assertion: assertion.clone(),
            records: vec![UpsertRecordInput {
                id: "record_001".to_string(),
                external_record_id: None,
                role: "user".to_string(),
                record_type: "message".to_string(),
                content: "继续实现可靠性方案".to_string(),
                structured_payload: Some(json!({"source_unit_revision": "source-unit-v2"})),
                metadata: Some(json!({
                    "turn_id": "turn_001",
                    "memory_project_id": "project_001"
                })),
                summary_status: None,
                summary_id: None,
                summarized_at: None,
                created_at: "2026-11-09T00:00:00Z".to_string(),
            }],
        },
    )
    .await?;
```

一个工具交换必须作为完整 Source Unit 写入，不能把 tool call、tool result 或最终 assistant output 拆成可被独立压缩的半条记录。

## 4. Compose 上下文

`ComposeContextPolicy` 的五个字段都是 token 硬预算，不接受条数限制或 `include_*` 开关。

调用方必须传已发布的 `model_capability_revision`。服务端按冻结的 context window、output reserve、tokenizer/estimator 和 compose policy 计算预算；必需内容超硬上限或检索前置条件不完整时直接失败。

返回值中的 `meta.budget_requested`、`meta.budget_effective`、`meta.budget_used`、`selected_count_by_pool`、`dropped_count_by_reason` 和 `needs_compaction` 用于解释本次选择。Compose 返回的内容始终已经受这些预算约束；`needs_compaction=true` 仅表示有更旧的原始记录未进入本次投影，应在后台启动摘要，不能因此阻塞当前模型调用或丢弃本次已选择的 Memory。

## 5. 总结与待处理重建

正常线程总结：

```rust
let response = client
    .run_thread_summary(
        "thread_001",
        "tenant_001",
        "project:project_001",
        &assertion,
    )
    .await?;
```

只对仍处于 pending 的原子 Source Unit 执行 durable rebuild：

```rust
let response = client
    .run_pending_summary_rebuild(
        "thread_001",
        "tenant_001",
        "project:project_001",
        &assertion,
    )
    .await?;

if response.accepted && response.running {
    println!("job_run_id={:?}", response.job_run_id);
}
```

`generated=false` 且 `running=true` 表示 Cloud Agent job 已持久化并等待继续执行；最终状态由 job 状态接口或管理平台读取。covered-range resummarize 是受审计的管理操作，不由普通 SDK 暴露。

## 6. 读取与分页

常用读取接口包括：

- `get_thread`、`list_threads`；
- `list_thread_records_page`；
- `list_thread_summaries`；
- `query_subject_memories`；
- `get_thread_active_summary_status`。

记录分页使用完整 cursor 对 `after_created_at + after_id`；两者必须同时提供。`items`、`total` 和 `has_more` 是当前分页契约。

## 7. 运行要求

- Memory Engine 默认地址：`http://127.0.0.1:7081`。
- SDK 方法返回 `Result<_, String>`。
- Configuration Center 的 Memory policy、Managed Agent Prompt、pipeline profile、模型能力和 tokenizer revision 缺失或校验失败时，服务启动或 generation 会 fail closed。
- Runtime Context Assertion 最长 TTL 为 300 秒；每次调用应使用与当前 scope 和 operation 匹配的新 assertion。
