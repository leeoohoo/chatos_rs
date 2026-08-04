// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde::{Deserialize, Serialize};

pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_MODE_ENV: &str = "TASK_RUNNER_RUN_DISPATCH_MODE";
pub const TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_ENV: &str = "TASK_RUNNER_CALLBACK_DELIVERY_MODE";
pub const TASK_RUNNER_QUEUE_RABBITMQ_URL_ENV: &str = "TASK_RUNNER_RABBITMQ_URL";
pub const TASK_RUNNER_QUEUE_RABBITMQ_EXCHANGE_ENV: &str = "TASK_RUNNER_RABBITMQ_EXCHANGE";
pub const TASK_RUNNER_QUEUE_RUN_DISPATCH_QUEUE_ENV: &str = "TASK_RUNNER_RUN_DISPATCH_QUEUE";
pub const TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_QUEUE_ENV: &str =
    "TASK_RUNNER_CALLBACK_DELIVERY_QUEUE";
pub const TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_ENV: &str =
    "TASK_RUNNER_RUN_EVENTS_PUBLISH_MODE";
pub const TASK_RUNNER_QUEUE_RUN_EVENTS_QUEUE_ENV: &str = "TASK_RUNNER_RUN_EVENTS_QUEUE";

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
    pub run_dispatch_queue: String,
    pub callback_delivery_queue: String,
    pub run_events_queue: String,
}

impl TaskQueueTopology {
    pub fn inline_defaults() -> Self {
        Self {
            run_dispatch_mode: TaskQueueMode::Inline,
            callback_delivery_mode: TaskQueueMode::Inline,
            run_events_publish_mode: TaskQueueMode::Inline,
            rabbitmq_url: None,
            rabbitmq_exchange: "task_runner".to_string(),
            run_dispatch_queue: "task_runner.run.dispatch".to_string(),
            callback_delivery_queue: "task_runner.callback.delivery".to_string(),
            run_events_queue: "task_runner.run.events".to_string(),
        }
    }

    pub fn from_managed_env() -> Result<Self, String> {
        let run_dispatch_mode = TaskQueueMode::parse(
            read_required_text(TASK_RUNNER_QUEUE_RUN_DISPATCH_MODE_ENV)?.as_str(),
        )?;
        let callback_delivery_mode = TaskQueueMode::parse(
            read_required_text(TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_MODE_ENV)?.as_str(),
        )?;
        let run_events_publish_mode = TaskQueueMode::parse(
            read_required_text(TASK_RUNNER_QUEUE_RUN_EVENTS_PUBLISH_MODE_ENV)?.as_str(),
        )?;
        let topology = Self {
            run_dispatch_mode,
            callback_delivery_mode,
            run_events_publish_mode,
            rabbitmq_url: Some(read_required_text(TASK_RUNNER_QUEUE_RABBITMQ_URL_ENV)?),
            rabbitmq_exchange: read_required_text(TASK_RUNNER_QUEUE_RABBITMQ_EXCHANGE_ENV)?,
            run_dispatch_queue: read_required_text(TASK_RUNNER_QUEUE_RUN_DISPATCH_QUEUE_ENV)?,
            callback_delivery_queue: read_required_text(
                TASK_RUNNER_QUEUE_CALLBACK_DELIVERY_QUEUE_ENV,
            )?,
            run_events_queue: read_required_text(TASK_RUNNER_QUEUE_RUN_EVENTS_QUEUE_ENV)?,
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
                "TASK_RUNNER_CALLBACK_DELIVERY_QUEUE",
                self.callback_delivery_queue.as_str(),
            ),
            (
                "TASK_RUNNER_RUN_EVENTS_QUEUE",
                self.run_events_queue.as_str(),
            ),
        ] {
            if value.trim().is_empty() {
                return Err(format!("{label} cannot be empty"));
            }
        }
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::{TaskQueueMode, TaskQueueTopology};

    #[test]
    fn defaults_to_inline_without_rabbitmq() {
        let topology = TaskQueueTopology {
            run_dispatch_mode: TaskQueueMode::Inline,
            callback_delivery_mode: TaskQueueMode::Inline,
            run_events_publish_mode: TaskQueueMode::Inline,
            rabbitmq_url: None,
            rabbitmq_exchange: "task_runner".to_string(),
            run_dispatch_queue: "task_runner.run.dispatch".to_string(),
            callback_delivery_queue: "task_runner.callback.delivery".to_string(),
            run_events_queue: "task_runner.run.events".to_string(),
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
            run_dispatch_queue: "task_runner.run.dispatch".to_string(),
            callback_delivery_queue: "task_runner.callback.delivery".to_string(),
            run_events_queue: "task_runner.run.events".to_string(),
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
}
