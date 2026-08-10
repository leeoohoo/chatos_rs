use chatos_queue_observability::{
    RabbitMqQueueInspector, RabbitMqQueueRuntimeStats, RabbitMqQueueSpec,
};
use serde_json::Value;

use super::*;
use crate::catalog::{
    MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY,
    MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE_CONFIG_KEY, MEMORY_ENGINE_RABBITMQ_URL_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE_CONFIG_KEY, MEMORY_ENGINE_ROLLUP_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_ROLLUP_RETRY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE_CONFIG_KEY, MEMORY_ENGINE_SUMMARY_QUEUE_CONFIG_KEY,
    MEMORY_ENGINE_SUMMARY_RETRY_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_QUEUE_CONFIG_KEY, PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL_CONFIG_KEY,
    PLUGIN_MANAGEMENT_CATALOG_RETRY_QUEUE_CONFIG_KEY, TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_POST_PROCESS_DEAD_LETTER_QUEUE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_POST_PROCESS_QUEUE_CONFIG_KEY,
    TASK_RUNNER_QUEUE_RUN_POST_PROCESS_RETRY_QUEUE_CONFIG_KEY,
};

#[derive(Debug, Clone)]
pub(super) struct ManagedQueueStream {
    service: &'static str,
    stream: &'static str,
    rabbitmq_url: String,
    main_queue: String,
    retry_queue: String,
    dead_letter_queue: String,
}

impl ManagedQueueStream {
    pub(super) fn rabbitmq_url(&self) -> &str {
        self.rabbitmq_url.as_str()
    }

    pub(super) async fn inspect_runtime(
        &self,
        inspector: &RabbitMqQueueInspector,
    ) -> RabbitMqQueueRuntimeStats {
        inspector
            .inspect(&[
                RabbitMqQueueSpec::new("main", self.main_queue.as_str()),
                RabbitMqQueueSpec::new("retry", self.retry_queue.as_str()),
                RabbitMqQueueSpec::new("dead_letter", self.dead_letter_queue.as_str()),
            ])
            .await
    }

    pub(super) fn into_response(self, runtime: RabbitMqQueueRuntimeStats) -> QueueOperationsStream {
        QueueOperationsStream {
            service: self.service.to_string(),
            stream: self.stream.to_string(),
            main_queue: self.main_queue,
            retry_queue: self.retry_queue,
            dead_letter_queue: self.dead_letter_queue,
            runtime,
        }
    }
}

pub(super) fn resolve_managed_streams(
    values: &BTreeMap<String, Value>,
) -> Result<Vec<ManagedQueueStream>, String> {
    let task_runner_url = required_text(values, TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY)?;
    let memory_engine_url = required_text(values, MEMORY_ENGINE_RABBITMQ_URL_CONFIG_KEY)?;
    let mcp_management_url =
        required_text(values, MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY)?;
    let plugin_management_url =
        required_text(values, PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL_CONFIG_KEY)?;
    Ok(vec![
        managed_stream(
            "task-runner",
            "run_post_process",
            task_runner_url,
            values,
            TASK_RUNNER_QUEUE_RUN_POST_PROCESS_QUEUE_CONFIG_KEY,
            TASK_RUNNER_QUEUE_RUN_POST_PROCESS_RETRY_QUEUE_CONFIG_KEY,
            TASK_RUNNER_QUEUE_RUN_POST_PROCESS_DEAD_LETTER_QUEUE_CONFIG_KEY,
        )?,
        managed_stream(
            "memory-engine",
            "summary",
            memory_engine_url.clone(),
            values,
            MEMORY_ENGINE_SUMMARY_QUEUE_CONFIG_KEY,
            MEMORY_ENGINE_SUMMARY_RETRY_QUEUE_CONFIG_KEY,
            MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE_CONFIG_KEY,
        )?,
        managed_stream(
            "memory-engine",
            "rollup",
            memory_engine_url.clone(),
            values,
            MEMORY_ENGINE_ROLLUP_QUEUE_CONFIG_KEY,
            MEMORY_ENGINE_ROLLUP_RETRY_QUEUE_CONFIG_KEY,
            MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE_CONFIG_KEY,
        )?,
        managed_stream(
            "memory-engine",
            "subject_memory",
            memory_engine_url,
            values,
            MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE_CONFIG_KEY,
            MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE_CONFIG_KEY,
            MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE_CONFIG_KEY,
        )?,
        managed_stream(
            "mcp-management",
            "async_tool",
            mcp_management_url,
            values,
            MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY,
            MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE_CONFIG_KEY,
            MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE_CONFIG_KEY,
        )?,
        managed_stream(
            "plugin-management",
            "catalog_sync",
            plugin_management_url,
            values,
            PLUGIN_MANAGEMENT_CATALOG_QUEUE_CONFIG_KEY,
            PLUGIN_MANAGEMENT_CATALOG_RETRY_QUEUE_CONFIG_KEY,
            PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY,
        )?,
    ])
}

fn managed_stream(
    service: &'static str,
    stream: &'static str,
    rabbitmq_url: String,
    values: &BTreeMap<String, Value>,
    main_queue_key: &str,
    retry_queue_key: &str,
    dead_letter_queue_key: &str,
) -> Result<ManagedQueueStream, String> {
    let main_queue = required_text(values, main_queue_key)?;
    let retry_queue = required_text(values, retry_queue_key)?;
    let dead_letter_queue = required_text(values, dead_letter_queue_key)?;
    if main_queue == retry_queue
        || main_queue == dead_letter_queue
        || retry_queue == dead_letter_queue
    {
        return Err(format!(
            "managed queue topology for {service}/{stream} must use distinct queues"
        ));
    }
    Ok(ManagedQueueStream {
        service,
        stream,
        rabbitmq_url,
        main_queue,
        retry_queue,
        dead_letter_queue,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_stream_resolution_requires_active_values_and_distinct_queues() {
        let mut values = BTreeMap::new();
        for key in [
            TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY,
            MEMORY_ENGINE_RABBITMQ_URL_CONFIG_KEY,
            MCP_MANAGEMENT_ASYNC_TOOL_RABBITMQ_URL_CONFIG_KEY,
            PLUGIN_MANAGEMENT_CATALOG_RABBITMQ_URL_CONFIG_KEY,
        ] {
            values.insert(key.to_string(), Value::String("amqp://managed".to_string()));
        }
        for (key, value) in [
            (
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_QUEUE_CONFIG_KEY,
                "task.main",
            ),
            (
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_RETRY_QUEUE_CONFIG_KEY,
                "task.retry",
            ),
            (
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_DEAD_LETTER_QUEUE_CONFIG_KEY,
                "task.dead",
            ),
            (MEMORY_ENGINE_SUMMARY_QUEUE_CONFIG_KEY, "memory.summary"),
            (
                MEMORY_ENGINE_SUMMARY_RETRY_QUEUE_CONFIG_KEY,
                "memory.summary.retry",
            ),
            (
                MEMORY_ENGINE_SUMMARY_DEAD_LETTER_QUEUE_CONFIG_KEY,
                "memory.summary.dead",
            ),
            (MEMORY_ENGINE_ROLLUP_QUEUE_CONFIG_KEY, "memory.rollup"),
            (
                MEMORY_ENGINE_ROLLUP_RETRY_QUEUE_CONFIG_KEY,
                "memory.rollup.retry",
            ),
            (
                MEMORY_ENGINE_ROLLUP_DEAD_LETTER_QUEUE_CONFIG_KEY,
                "memory.rollup.dead",
            ),
            (
                MEMORY_ENGINE_SUBJECT_MEMORY_QUEUE_CONFIG_KEY,
                "memory.subject",
            ),
            (
                MEMORY_ENGINE_SUBJECT_MEMORY_RETRY_QUEUE_CONFIG_KEY,
                "memory.subject.retry",
            ),
            (
                MEMORY_ENGINE_SUBJECT_MEMORY_DEAD_LETTER_QUEUE_CONFIG_KEY,
                "memory.subject.dead",
            ),
            (
                MCP_MANAGEMENT_ASYNC_TOOL_DISPATCH_QUEUE_CONFIG_KEY,
                "mcp.main",
            ),
            (
                MCP_MANAGEMENT_ASYNC_TOOL_RETRY_QUEUE_CONFIG_KEY,
                "mcp.retry",
            ),
            (
                MCP_MANAGEMENT_ASYNC_TOOL_DEAD_LETTER_QUEUE_CONFIG_KEY,
                "mcp.dead",
            ),
            (PLUGIN_MANAGEMENT_CATALOG_QUEUE_CONFIG_KEY, "plugin.main"),
            (
                PLUGIN_MANAGEMENT_CATALOG_RETRY_QUEUE_CONFIG_KEY,
                "plugin.retry",
            ),
            (
                PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY,
                "plugin.dead",
            ),
        ] {
            values.insert(key.to_string(), Value::String(value.to_string()));
        }

        let streams = resolve_managed_streams(&values).expect("resolve managed streams");
        assert_eq!(streams.len(), 6);
        assert_eq!(streams[0].service, "task-runner");
        assert_eq!(streams[5].stream, "catalog_sync");

        values.insert(
            PLUGIN_MANAGEMENT_CATALOG_DEAD_LETTER_QUEUE_CONFIG_KEY.to_string(),
            Value::String("plugin.main".to_string()),
        );
        assert!(resolve_managed_streams(&values).is_err());
    }

    #[test]
    fn managed_stream_resolution_does_not_use_missing_value_defaults() {
        let values = BTreeMap::new();
        let error = resolve_managed_streams(&values).expect_err("missing values must fail");
        assert!(error.contains(TASK_RUNNER_QUEUE_RABBITMQ_URL_CONFIG_KEY));
    }
}
