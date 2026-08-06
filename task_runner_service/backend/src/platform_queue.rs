// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_MODE_ENV: &str = "TASK_RUNNER_RUN_DISPATCH_MODE";
pub const TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_ENV: &str = "TASK_RUNNER_CALLBACK_DELIVERY_MODE";
pub const TASK_RUNNER_QUEUE_RABBITMQ_URL_ENV: &str = "TASK_RUNNER_RABBITMQ_URL";
pub const TASK_RUNNER_QUEUE_RABBITMQ_EXCHANGE_ENV: &str = "TASK_RUNNER_RABBITMQ_EXCHANGE";
pub const TASK_RUNNER_QUEUE_RABBITMQ_RECONNECT_MS_ENV: &str = "TASK_RUNNER_RABBITMQ_RECONNECT_MS";
pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_QUEUE_ENV: &str = "TASK_RUNNER_RUN_DISPATCH_QUEUE";
pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_RETRY_QUEUE_ENV: &str =
    "TASK_RUNNER_RUN_DISPATCH_RETRY_QUEUE";
pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_RETRY_DELAY_MS_ENV: &str =
    "TASK_RUNNER_RUN_DISPATCH_RETRY_DELAY_MS";
pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_OUTBOX_RECONCILE_MS_ENV: &str =
    "TASK_RUNNER_RUN_DISPATCH_OUTBOX_RECONCILE_MS";
pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_OUTBOX_BATCH_SIZE_ENV: &str =
    "TASK_RUNNER_RUN_DISPATCH_OUTBOX_BATCH_SIZE";
pub const TASK_RUNNER_QUEUE_WORKER_CONTROL_QUEUE_PREFIX_ENV: &str =
    "TASK_RUNNER_WORKER_CONTROL_QUEUE_PREFIX";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_QUEUE_ENV: &str = "TASK_RUNNER_RUN_POST_PROCESS_QUEUE";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_RETRY_QUEUE_ENV: &str =
    "TASK_RUNNER_RUN_POST_PROCESS_RETRY_QUEUE";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_DEAD_LETTER_QUEUE_ENV: &str =
    "TASK_RUNNER_RUN_POST_PROCESS_DEAD_LETTER_QUEUE";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_MAX_DELIVERY_ATTEMPTS_ENV: &str =
    "TASK_RUNNER_RUN_POST_PROCESS_MAX_DELIVERY_ATTEMPTS";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_RETRY_DELAY_MS_ENV: &str =
    "TASK_RUNNER_RUN_POST_PROCESS_RETRY_DELAY_MS";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_OUTBOX_RECONCILE_MS_ENV: &str =
    "TASK_RUNNER_RUN_POST_PROCESS_OUTBOX_RECONCILE_MS";
pub const TASK_RUNNER_QUEUE_RUN_POST_PROCESS_OUTBOX_BATCH_SIZE_ENV: &str =
    "TASK_RUNNER_RUN_POST_PROCESS_OUTBOX_BATCH_SIZE";
pub const TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_QUEUE_ENV: &str =
    "TASK_RUNNER_CALLBACK_DELIVERY_QUEUE";
pub const TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_ENV: &str =
    "TASK_RUNNER_RUN_EVENTS_PUBLISH_MODE";
pub const TASK_RUNNER_QUEUE_RUN_EVENTS_ROUTING_KEY_ENV: &str = "TASK_RUNNER_RUN_EVENTS_ROUTING_KEY";
pub const TASK_RUNNER_MCP_RESULT_QUEUE_PREFIX_ENV: &str = "TASK_RUNNER_MCP_RESULT_QUEUE_PREFIX";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskQueueMode {
    Inline,
    RabbitMq,
}

impl TaskQueueMode {
    fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "inline" => Ok(Self::Inline),
            "rabbitmq" | "rabbit_mq" | "amqp" => Ok(Self::RabbitMq),
            other => Err(format!(
                "invalid task runner queue mode {other}: expected inline or rabbitmq"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::RabbitMq => "rabbitmq",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskQueueTopology {
    pub run_dispatch_mode: TaskQueueMode,
    pub callback_delivery_mode: TaskQueueMode,
    pub run_events_publish_mode: TaskQueueMode,
    pub rabbitmq_url: Option<String>,
    pub rabbitmq_exchange: String,
    pub rabbitmq_reconnect_delay: Duration,
    pub run_dispatch_queue: String,
    pub run_dispatch_retry_queue: String,
    pub run_dispatch_retry_delay: Duration,
    pub run_dispatch_outbox_reconcile_interval: Duration,
    pub run_dispatch_outbox_batch_size: usize,
    pub worker_control_queue_prefix: String,
    pub run_post_process_queue: String,
    pub run_post_process_retry_queue: String,
    pub run_post_process_dead_letter_queue: String,
    pub run_post_process_max_delivery_attempts: u32,
    pub run_post_process_retry_delay: Duration,
    pub run_post_process_outbox_reconcile_interval: Duration,
    pub run_post_process_outbox_batch_size: usize,
    pub callback_delivery_queue: String,
    pub run_events_routing_key: String,
    pub mcp_result_queue_prefix: String,
}

impl TaskQueueTopology {
    pub fn inline_defaults() -> Self {
        Self {
            run_dispatch_mode: TaskQueueMode::Inline,
            callback_delivery_mode: TaskQueueMode::Inline,
            run_events_publish_mode: TaskQueueMode::Inline,
            rabbitmq_url: None,
            rabbitmq_exchange: "task_runner".to_string(),
            rabbitmq_reconnect_delay: Duration::from_secs(3),
            run_dispatch_queue: "task_runner.run.dispatch".to_string(),
            run_dispatch_retry_queue: "task_runner.run.dispatch.retry".to_string(),
            run_dispatch_retry_delay: Duration::from_secs(1),
            run_dispatch_outbox_reconcile_interval: Duration::from_secs(5),
            run_dispatch_outbox_batch_size: 100,
            worker_control_queue_prefix: "task_runner.worker.control".to_string(),
            run_post_process_queue: "task_runner.run.post_process".to_string(),
            run_post_process_retry_queue: "task_runner.run.post_process.retry".to_string(),
            run_post_process_dead_letter_queue: "task_runner.run.post_process.dead".to_string(),
            run_post_process_max_delivery_attempts: 8,
            run_post_process_retry_delay: Duration::from_secs(5),
            run_post_process_outbox_reconcile_interval: Duration::from_secs(5),
            run_post_process_outbox_batch_size: 100,
            callback_delivery_queue: "task_runner.callback.delivery".to_string(),
            run_events_routing_key: "task_runner.run.events.broadcast".to_string(),
            mcp_result_queue_prefix: "task_runner.mcp.results".to_string(),
        }
    }

    pub fn from_managed_env() -> Result<Self, String> {
        let run_dispatch_mode = read_required_text(TASK_RUNNER_QUEUE_RUN_DISPATCH_MODE_ENV)?;
        if run_dispatch_mode != "rabbitmq" {
            return Err(format!(
                "{TASK_RUNNER_QUEUE_RUN_DISPATCH_MODE_ENV} only supports rabbitmq"
            ));
        }
        let callback_delivery_mode = TaskQueueMode::parse(
            read_required_text(TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_ENV)?.as_str(),
        )?;
        let run_events_publish_mode =
            read_required_text(TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_ENV)?;
        if run_events_publish_mode != "rabbitmq" {
            return Err(format!(
                "{TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_ENV} only supports rabbitmq"
            ));
        }
        let topology = Self {
            run_dispatch_mode: TaskQueueMode::RabbitMq,
            callback_delivery_mode,
            run_events_publish_mode: TaskQueueMode::RabbitMq,
            rabbitmq_url: Some(read_required_text(TASK_RUNNER_QUEUE_RABBITMQ_URL_ENV)?),
            rabbitmq_exchange: read_required_text(TASK_RUNNER_QUEUE_RABBITMQ_EXCHANGE_ENV)?,
            rabbitmq_reconnect_delay: Duration::from_millis(read_required_u64(
                TASK_RUNNER_QUEUE_RABBITMQ_RECONNECT_MS_ENV,
            )?),
            run_dispatch_queue: read_required_text(TASK_RUNNER_QUEUE_RUN_DISPATCH_QUEUE_ENV)?,
            run_dispatch_retry_queue: read_required_text(
                TASK_RUNNER_QUEUE_RUN_DISPATCH_RETRY_QUEUE_ENV,
            )?,
            run_dispatch_retry_delay: Duration::from_millis(read_required_u64(
                TASK_RUNNER_QUEUE_RUN_DISPATCH_RETRY_DELAY_MS_ENV,
            )?),
            run_dispatch_outbox_reconcile_interval: Duration::from_millis(read_required_u64(
                TASK_RUNNER_QUEUE_RUN_DISPATCH_OUTBOX_RECONCILE_MS_ENV,
            )?),
            run_dispatch_outbox_batch_size: usize::try_from(read_required_u64(
                TASK_RUNNER_QUEUE_RUN_DISPATCH_OUTBOX_BATCH_SIZE_ENV,
            )?)
            .map_err(|_| "TASK_RUNNER_RUN_DISPATCH_OUTBOX_BATCH_SIZE is too large".to_string())?,
            worker_control_queue_prefix: read_required_text(
                TASK_RUNNER_QUEUE_WORKER_CONTROL_QUEUE_PREFIX_ENV,
            )?,
            run_post_process_queue: read_required_text(
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_QUEUE_ENV,
            )?,
            run_post_process_retry_queue: read_required_text(
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_RETRY_QUEUE_ENV,
            )?,
            run_post_process_dead_letter_queue: read_required_text(
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_DEAD_LETTER_QUEUE_ENV,
            )?,
            run_post_process_max_delivery_attempts: u32::try_from(read_required_u64(
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_MAX_DELIVERY_ATTEMPTS_ENV,
            )?)
            .map_err(|_| {
                "TASK_RUNNER_RUN_POST_PROCESS_MAX_DELIVERY_ATTEMPTS is too large".to_string()
            })?,
            run_post_process_retry_delay: Duration::from_millis(read_required_u64(
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_RETRY_DELAY_MS_ENV,
            )?),
            run_post_process_outbox_reconcile_interval: Duration::from_millis(read_required_u64(
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_OUTBOX_RECONCILE_MS_ENV,
            )?),
            run_post_process_outbox_batch_size: usize::try_from(read_required_u64(
                TASK_RUNNER_QUEUE_RUN_POST_PROCESS_OUTBOX_BATCH_SIZE_ENV,
            )?)
            .map_err(|_| {
                "TASK_RUNNER_RUN_POST_PROCESS_OUTBOX_BATCH_SIZE is too large".to_string()
            })?,
            callback_delivery_queue: read_required_text(
                TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_QUEUE_ENV,
            )?,
            run_events_routing_key: read_required_text(
                TASK_RUNNER_QUEUE_RUN_EVENTS_ROUTING_KEY_ENV,
            )?,
            mcp_result_queue_prefix: read_required_text(TASK_RUNNER_MCP_RESULT_QUEUE_PREFIX_ENV)?,
        };
        topology.validate()?;
        Ok(topology)
    }

    pub fn uses_rabbitmq(&self) -> bool {
        self.run_dispatch_mode == TaskQueueMode::RabbitMq
            || self.callback_delivery_mode == TaskQueueMode::RabbitMq
            || self.run_events_publish_mode == TaskQueueMode::RabbitMq
    }

    pub fn publishes_run_events_to_rabbitmq(&self) -> bool {
        self.run_events_publish_mode == TaskQueueMode::RabbitMq
    }

    fn validate(&self) -> Result<(), String> {
        if self.uses_rabbitmq() && self.rabbitmq_url.is_none() {
            return Err(
                "TASK_RUNNER_RABBITMQ_URL is required when a task runner queue mode uses RabbitMQ"
                    .to_string(),
            );
        }
        for (label, value) in [
            (
                "TASK_RUNNER_RABBITMQ_EXCHANGE",
                self.rabbitmq_exchange.as_str(),
            ),
            (
                "TASK_RUNNER_RUN_DISPATCH_QUEUE",
                self.run_dispatch_queue.as_str(),
            ),
            (
                "TASK_RUNNER_RUN_DISPATCH_RETRY_QUEUE",
                self.run_dispatch_retry_queue.as_str(),
            ),
            (
                "TASK_RUNNER_CALLBACK_DELIVERY_QUEUE",
                self.callback_delivery_queue.as_str(),
            ),
            (
                "TASK_RUNNER_WORKER_CONTROL_QUEUE_PREFIX",
                self.worker_control_queue_prefix.as_str(),
            ),
            (
                "TASK_RUNNER_RUN_POST_PROCESS_QUEUE",
                self.run_post_process_queue.as_str(),
            ),
            (
                "TASK_RUNNER_RUN_POST_PROCESS_RETRY_QUEUE",
                self.run_post_process_retry_queue.as_str(),
            ),
            (
                "TASK_RUNNER_RUN_POST_PROCESS_DEAD_LETTER_QUEUE",
                self.run_post_process_dead_letter_queue.as_str(),
            ),
            (
                "TASK_RUNNER_RUN_EVENTS_ROUTING_KEY",
                self.run_events_routing_key.as_str(),
            ),
            (
                "TASK_RUNNER_MCP_RESULT_QUEUE_PREFIX",
                self.mcp_result_queue_prefix.as_str(),
            ),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{label} cannot be empty"));
            }
        }
        if self.run_dispatch_retry_delay.is_zero() {
            return Err(
                "TASK_RUNNER_RUN_DISPATCH_RETRY_DELAY_MS must be greater than zero".to_string(),
            );
        }
        if !(Duration::from_millis(100)..=Duration::from_secs(60))
            .contains(&self.rabbitmq_reconnect_delay)
        {
            return Err(
                "TASK_RUNNER_RABBITMQ_RECONNECT_MS must be between 100 and 60000".to_string(),
            );
        }
        if self.run_dispatch_outbox_reconcile_interval.is_zero() {
            return Err(
                "TASK_RUNNER_RUN_DISPATCH_OUTBOX_RECONCILE_MS must be greater than zero"
                    .to_string(),
            );
        }
        if self.run_dispatch_outbox_batch_size == 0 {
            return Err(
                "TASK_RUNNER_RUN_DISPATCH_OUTBOX_BATCH_SIZE must be greater than zero".to_string(),
            );
        }
        if self.run_post_process_retry_delay.is_zero() {
            return Err(
                "TASK_RUNNER_RUN_POST_PROCESS_RETRY_DELAY_MS must be greater than zero".to_string(),
            );
        }
        if self.run_post_process_max_delivery_attempts == 0 {
            return Err(
                "TASK_RUNNER_RUN_POST_PROCESS_MAX_DELIVERY_ATTEMPTS must be greater than zero"
                    .to_string(),
            );
        }
        if self.run_post_process_outbox_reconcile_interval.is_zero() {
            return Err(
                "TASK_RUNNER_RUN_POST_PROCESS_OUTBOX_RECONCILE_MS must be greater than zero"
                    .to_string(),
            );
        }
        if self.run_post_process_outbox_batch_size == 0 {
            return Err(
                "TASK_RUNNER_RUN_POST_PROCESS_OUTBOX_BATCH_SIZE must be greater than zero"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub fn mcp_result_queue_config(
        &self,
        worker_id: &str,
    ) -> Result<chatos_mcp_runtime::McpInvocationResultQueueConfig, String> {
        let rabbitmq_url = self.rabbitmq_url.clone().ok_or_else(|| {
            "TASK_RUNNER_RABBITMQ_URL is required for MCP result events".to_string()
        })?;
        let instance = rabbitmq_queue_component(worker_id)?;
        Ok(chatos_mcp_runtime::McpInvocationResultQueueConfig {
            rabbitmq_url,
            queue_name: format!("{}.{}", self.mcp_result_queue_prefix, instance),
        })
    }

    pub fn worker_control_queue_name(&self, worker_id: &str) -> Result<String, String> {
        Ok(format!(
            "{}.{}",
            self.worker_control_queue_prefix,
            rabbitmq_queue_component(worker_id)?
        ))
    }
}

fn rabbitmq_queue_component(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 100 {
        return Err("Task Runner worker id is invalid for RabbitMQ queue routing".to_string());
    }
    Ok(value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect())
}

fn read_text(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_required_text(key: &str) -> Result<String, String> {
    read_text(key).ok_or_else(|| {
        format!(
            "{key} is required and must be supplied by Configuration Center managed task runner queue settings"
        )
    })
}

fn read_required_u64(key: &str) -> Result<u64, String> {
    read_required_text(key)?
        .parse::<u64>()
        .map_err(|err| format!("{key} must be an unsigned integer: {err}"))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{TaskQueueMode, TaskQueueTopology};

    #[test]
    fn defaults_to_inline_without_rabbitmq() {
        let topology = TaskQueueTopology {
            run_dispatch_mode: TaskQueueMode::Inline,
            callback_delivery_mode: TaskQueueMode::Inline,
            run_events_publish_mode: TaskQueueMode::Inline,
            rabbitmq_url: None,
            rabbitmq_exchange: "task_runner".to_string(),
            rabbitmq_reconnect_delay: Duration::from_secs(3),
            run_dispatch_queue: "task_runner.run.dispatch".to_string(),
            run_dispatch_retry_queue: "task_runner.run.dispatch.retry".to_string(),
            run_dispatch_retry_delay: Duration::from_secs(1),
            run_dispatch_outbox_reconcile_interval: Duration::from_secs(5),
            run_dispatch_outbox_batch_size: 100,
            worker_control_queue_prefix: "task_runner.worker.control".to_string(),
            run_post_process_queue: "task_runner.run.post_process".to_string(),
            run_post_process_retry_queue: "task_runner.run.post_process.retry".to_string(),
            run_post_process_dead_letter_queue: "task_runner.run.post_process.dead".to_string(),
            run_post_process_max_delivery_attempts: 8,
            run_post_process_retry_delay: Duration::from_secs(5),
            run_post_process_outbox_reconcile_interval: Duration::from_secs(5),
            run_post_process_outbox_batch_size: 100,
            callback_delivery_queue: "task_runner.callback.delivery".to_string(),
            run_events_routing_key: "task_runner.run.events.broadcast".to_string(),
            mcp_result_queue_prefix: "task_runner.mcp.results".to_string(),
        };
        assert!(!topology.uses_rabbitmq());
    }

    #[test]
    fn rabbitmq_mode_requires_url() {
        let topology = TaskQueueTopology {
            run_dispatch_mode: TaskQueueMode::RabbitMq,
            callback_delivery_mode: TaskQueueMode::Inline,
            run_events_publish_mode: TaskQueueMode::Inline,
            rabbitmq_url: None,
            rabbitmq_exchange: "task_runner".to_string(),
            rabbitmq_reconnect_delay: Duration::from_secs(3),
            run_dispatch_queue: "task_runner.run.dispatch".to_string(),
            run_dispatch_retry_queue: "task_runner.run.dispatch.retry".to_string(),
            run_dispatch_retry_delay: Duration::from_secs(1),
            run_dispatch_outbox_reconcile_interval: Duration::from_secs(5),
            run_dispatch_outbox_batch_size: 100,
            worker_control_queue_prefix: "task_runner.worker.control".to_string(),
            run_post_process_queue: "task_runner.run.post_process".to_string(),
            run_post_process_retry_queue: "task_runner.run.post_process.retry".to_string(),
            run_post_process_dead_letter_queue: "task_runner.run.post_process.dead".to_string(),
            run_post_process_max_delivery_attempts: 8,
            run_post_process_retry_delay: Duration::from_secs(5),
            run_post_process_outbox_reconcile_interval: Duration::from_secs(5),
            run_post_process_outbox_batch_size: 100,
            callback_delivery_queue: "task_runner.callback.delivery".to_string(),
            run_events_routing_key: "task_runner.run.events.broadcast".to_string(),
            mcp_result_queue_prefix: "task_runner.mcp.results".to_string(),
        };
        assert!(topology.uses_rabbitmq());
        assert!(topology.validate().is_err());
    }

    #[test]
    fn queue_mode_parser_rejects_unknown_values_instead_of_falling_back() {
        assert_eq!(
            TaskQueueMode::parse("inline").expect("inline mode"),
            TaskQueueMode::Inline
        );
        assert_eq!(
            TaskQueueMode::parse("rabbitmq").expect("rabbitmq mode"),
            TaskQueueMode::RabbitMq
        );
        assert!(TaskQueueMode::parse("typo").is_err());
        assert!(TaskQueueMode::parse("").is_err());
    }

    #[test]
    fn rabbitmq_reconnect_delay_is_strictly_bounded() {
        let mut topology = TaskQueueTopology::inline_defaults();

        topology.rabbitmq_reconnect_delay = Duration::from_millis(99);
        assert!(topology.validate().is_err());

        topology.rabbitmq_reconnect_delay = Duration::from_millis(100);
        assert!(topology.validate().is_ok());

        topology.rabbitmq_reconnect_delay = Duration::from_secs(60);
        assert!(topology.validate().is_ok());

        topology.rabbitmq_reconnect_delay = Duration::from_millis(60_001);
        assert!(topology.validate().is_err());
    }

    #[test]
    fn worker_control_queue_name_normalizes_worker_id() {
        let topology = TaskQueueTopology::inline_defaults();

        assert_eq!(
            topology
                .worker_control_queue_name(" worker/01:primary ")
                .expect("worker control queue"),
            "task_runner.worker.control.worker_01_primary"
        );
        assert!(topology.worker_control_queue_name("   ").is_err());
    }
}
