// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use lapin::{
    options::QueueDeclareOptions, types::FieldTable, Channel, Connection, ConnectionProperties,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct RabbitMqQueueSpec {
    pub role: &'static str,
    pub name: String,
}

impl RabbitMqQueueSpec {
    pub fn new(role: &'static str, name: impl Into<String>) -> Self {
        Self {
            role,
            name: name.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RabbitMqQueueRuntimeStats {
    pub enabled: bool,
    pub available: bool,
    pub queues: Vec<RabbitMqQueueDepth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl RabbitMqQueueRuntimeStats {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            available: false,
            queues: Vec::new(),
            error: None,
        }
    }

    fn unavailable() -> Self {
        Self {
            enabled: true,
            available: false,
            queues: Vec::new(),
            error: Some("rabbitmq_queue_inspection_unavailable".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RabbitMqQueueDepth {
    pub role: String,
    pub name: String,
    pub messages: u32,
    pub consumers: u32,
}

pub fn render_prometheus_metrics(service: &str, stats: &RabbitMqQueueRuntimeStats) -> String {
    let service = escape_prometheus_label(service);
    let mut output = String::from(
        "# HELP chatos_rabbitmq_queue_observability_enabled Whether RabbitMQ queue observability is enabled.\n\
# TYPE chatos_rabbitmq_queue_observability_enabled gauge\n",
    );
    output.push_str(
        format!(
            "chatos_rabbitmq_queue_observability_enabled{{service=\"{service}\"}} {}\n",
            u8::from(stats.enabled)
        )
        .as_str(),
    );
    output.push_str(
        "# HELP chatos_rabbitmq_queue_observability_available Whether RabbitMQ queue state was available for this scrape.\n\
# TYPE chatos_rabbitmq_queue_observability_available gauge\n",
    );
    output.push_str(
        format!(
            "chatos_rabbitmq_queue_observability_available{{service=\"{service}\"}} {}\n",
            u8::from(stats.available)
        )
        .as_str(),
    );
    output.push_str(
        "# HELP chatos_rabbitmq_queue_messages Ready messages currently stored in a RabbitMQ queue.\n\
# TYPE chatos_rabbitmq_queue_messages gauge\n\
# HELP chatos_rabbitmq_queue_consumers Active consumers currently attached to a RabbitMQ queue.\n\
# TYPE chatos_rabbitmq_queue_consumers gauge\n",
    );
    for queue in &stats.queues {
        let role = escape_prometheus_label(queue.role.as_str());
        let name = escape_prometheus_label(queue.name.as_str());
        let labels = format!("service=\"{service}\",role=\"{role}\",queue=\"{name}\"");
        output.push_str(
            format!(
                "chatos_rabbitmq_queue_messages{{{labels}}} {}\n",
                queue.messages
            )
            .as_str(),
        );
        output.push_str(
            format!(
                "chatos_rabbitmq_queue_consumers{{{labels}}} {}\n",
                queue.consumers
            )
            .as_str(),
        );
    }
    output
}

fn escape_prometheus_label(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[derive(Clone)]
pub struct RabbitMqQueueInspector {
    rabbitmq_url: Arc<str>,
    connection: Arc<Mutex<Option<Arc<RabbitMqInspectionConnection>>>>,
}

struct RabbitMqInspectionConnection {
    _connection: Connection,
    channel: Channel,
}

impl RabbitMqQueueInspector {
    pub fn new(rabbitmq_url: impl Into<String>) -> Result<Self, String> {
        let rabbitmq_url = rabbitmq_url.into();
        if rabbitmq_url.trim().is_empty() {
            return Err("RabbitMQ queue inspector URL is required".to_string());
        }
        Ok(Self {
            rabbitmq_url: Arc::from(rabbitmq_url),
            connection: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn inspect(&self, specs: &[RabbitMqQueueSpec]) -> RabbitMqQueueRuntimeStats {
        let connection = match self.connection().await {
            Ok(connection) => connection,
            Err(()) => return RabbitMqQueueRuntimeStats::unavailable(),
        };
        let mut queues = Vec::with_capacity(specs.len());
        for spec in specs {
            match inspect_queue(&connection.channel, spec).await {
                Ok(queue) => queues.push(queue),
                Err(()) => {
                    let mut guard = self.connection.lock().await;
                    if guard
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, &connection))
                    {
                        *guard = None;
                    }
                    return RabbitMqQueueRuntimeStats::unavailable();
                }
            }
        }
        RabbitMqQueueRuntimeStats {
            enabled: true,
            available: true,
            queues,
            error: None,
        }
    }

    async fn connection(&self) -> Result<Arc<RabbitMqInspectionConnection>, ()> {
        let mut guard = self.connection.lock().await;
        if let Some(connection) = guard.as_ref() {
            return Ok(connection.clone());
        }
        let connection =
            Connection::connect(self.rabbitmq_url.as_ref(), ConnectionProperties::default())
                .await
                .map_err(|_| ())?;
        let channel = connection.create_channel().await.map_err(|_| ())?;
        let connection = Arc::new(RabbitMqInspectionConnection {
            _connection: connection,
            channel,
        });
        *guard = Some(connection.clone());
        Ok(connection)
    }
}

async fn inspect_queue(
    channel: &Channel,
    spec: &RabbitMqQueueSpec,
) -> Result<RabbitMqQueueDepth, ()> {
    let queue = channel
        .queue_declare(
            spec.name.as_str(),
            QueueDeclareOptions {
                passive: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|_| ())?;
    Ok(RabbitMqQueueDepth {
        role: spec.role.to_string(),
        name: spec.name.clone(),
        messages: queue.message_count(),
        consumers: queue.consumer_count(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspector_rejects_empty_urls() {
        assert!(RabbitMqQueueInspector::new("  ").is_err());
    }

    #[test]
    fn disabled_stats_do_not_claim_rabbitmq_availability() {
        let stats = RabbitMqQueueRuntimeStats::disabled();
        assert!(!stats.enabled);
        assert!(!stats.available);
        assert!(stats.queues.is_empty());
        assert!(stats.error.is_none());
    }

    #[test]
    fn prometheus_metrics_escape_labels_and_include_queue_depths() {
        let metrics = render_prometheus_metrics(
            "plugin\"management",
            &RabbitMqQueueRuntimeStats {
                enabled: true,
                available: true,
                queues: vec![RabbitMqQueueDepth {
                    role: "catalog\\sync".to_string(),
                    name: "plugin.catalog\nsync".to_string(),
                    messages: 7,
                    consumers: 2,
                }],
                error: None,
            },
        );

        assert!(metrics.contains("service=\"plugin\\\"management\""));
        assert!(metrics.contains("role=\"catalog\\\\sync\""));
        assert!(metrics.contains("queue=\"plugin.catalog\\nsync\""));
        assert!(metrics.contains("chatos_rabbitmq_queue_messages{"));
        assert!(metrics.ends_with(" 2\n"));
    }
}
