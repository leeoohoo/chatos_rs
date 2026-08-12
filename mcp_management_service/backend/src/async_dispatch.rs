// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{error::Error, fmt};

use chatos_queue_observability::{
    RabbitMqQueueInspector, RabbitMqQueueRuntimeStats, RabbitMqQueueSpec,
};
use lapin::{options::BasicPublishOptions, publisher_confirm::Confirmation, BasicProperties};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::config::{AsyncToolDispatchMode, AsyncToolDispatchTopology};
use crate::state::AppState;

const RABBITMQ_CONSUMER_TAG: &str = "mcp-management-async-tool-dispatch";
const RABBITMQ_CANCELLATION_CONSUMER_TAG: &str = "mcp-management-invocation-cancellations";

mod rabbitmq;
#[cfg(test)]
mod tests;

#[cfg(test)]
use rabbitmq::{dispatch_queue_arguments, ensure_publish_confirmed};
use rabbitmq::{
    open_rabbitmq_publisher, run_cancellation_consumer_loop, run_rabbitmq_consumer_loop,
    unavailable_rabbitmq_queue_stats, RabbitMqPublisher,
};

#[derive(Debug, Serialize, Deserialize)]
struct InvocationCancellationEvent {
    invocation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsyncToolEnqueueError {
    CapacityExhausted,
    Unavailable(String),
}

impl fmt::Display for AsyncToolEnqueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapacityExhausted => formatter.write_str(
                "RabbitMQ rejected the MCP async tool event because queue capacity is exhausted",
            ),
            Self::Unavailable(message) => formatter.write_str(message),
        }
    }
}

impl Error for AsyncToolEnqueueError {}

#[derive(Debug, Clone, Serialize)]
pub struct AsyncToolDispatchRuntimeStats {
    pub consumer_connected: bool,
    pub cancellation_consumer_connected: bool,
    pub cancellation_publisher_connected: bool,
}

#[derive(Default)]
struct AsyncToolDispatchMetrics {
    consumer_connected: AtomicBool,
    cancellation_consumer_connected: AtomicBool,
    cancellation_publisher_connected: AtomicBool,
}

#[derive(Clone)]
pub struct AsyncToolDispatch {
    topology: AsyncToolDispatchTopology,
    rabbitmq_publisher: Arc<Mutex<Option<Arc<RabbitMqPublisher>>>>,
    rabbitmq_inspector: Arc<Mutex<Option<Arc<RabbitMqQueueInspector>>>>,
    metrics: Arc<AsyncToolDispatchMetrics>,
}

impl AsyncToolDispatch {
    pub fn new(topology: AsyncToolDispatchTopology) -> Self {
        Self {
            topology,
            rabbitmq_publisher: Arc::new(Mutex::new(None)),
            rabbitmq_inspector: Arc::new(Mutex::new(None)),
            metrics: Arc::new(AsyncToolDispatchMetrics::default()),
        }
    }

    pub fn topology(&self) -> &AsyncToolDispatchTopology {
        &self.topology
    }

    pub fn runtime_stats(&self) -> AsyncToolDispatchRuntimeStats {
        AsyncToolDispatchRuntimeStats {
            consumer_connected: self.metrics.consumer_connected.load(Ordering::Relaxed),
            cancellation_consumer_connected: self
                .metrics
                .cancellation_consumer_connected
                .load(Ordering::Relaxed),
            cancellation_publisher_connected: self
                .metrics
                .cancellation_publisher_connected
                .load(Ordering::Relaxed),
        }
    }

    pub async fn rabbitmq_queue_stats(&self) -> RabbitMqQueueRuntimeStats {
        if self.topology.mode != AsyncToolDispatchMode::RabbitMq {
            return RabbitMqQueueRuntimeStats::disabled();
        }
        let inspector = match self.rabbitmq_inspector().await {
            Ok(inspector) => inspector,
            Err(()) => return unavailable_rabbitmq_queue_stats(),
        };
        let Some(dispatch_queue) = self.topology.queue_name.as_ref() else {
            return unavailable_rabbitmq_queue_stats();
        };
        let Some(retry_queue) = self.topology.retry_queue_name.as_ref() else {
            return unavailable_rabbitmq_queue_stats();
        };
        let Some(dead_letter_queue) = self.topology.dead_letter_queue_name.as_ref() else {
            return unavailable_rabbitmq_queue_stats();
        };
        inspector
            .inspect(&[
                RabbitMqQueueSpec::new("dispatch", dispatch_queue),
                RabbitMqQueueSpec::new("retry", retry_queue),
                RabbitMqQueueSpec::new("dead_letter", dead_letter_queue),
            ])
            .await
    }

    async fn rabbitmq_inspector(&self) -> Result<Arc<RabbitMqQueueInspector>, ()> {
        let mut guard = self.rabbitmq_inspector.lock().await;
        if let Some(inspector) = guard.as_ref() {
            return Ok(inspector.clone());
        }
        let rabbitmq_url = self.topology.rabbitmq_url.as_deref().ok_or(())?;
        let inspector = Arc::new(RabbitMqQueueInspector::new(rabbitmq_url).map_err(|_| ())?);
        *guard = Some(inspector.clone());
        Ok(inspector)
    }

    pub(crate) fn set_consumer_connected(&self, connected: bool) {
        self.metrics
            .consumer_connected
            .store(connected, Ordering::Relaxed);
    }

    fn set_cancellation_consumer_connected(&self, connected: bool) {
        self.metrics
            .cancellation_consumer_connected
            .store(connected, Ordering::Relaxed);
    }

    async fn rabbitmq_publisher(&self) -> Result<Arc<RabbitMqPublisher>, AsyncToolEnqueueError> {
        let mut guard = self.rabbitmq_publisher.lock().await;
        if let Some(publisher) = guard.as_ref() {
            return Ok(publisher.clone());
        }
        let publisher = Arc::new(open_rabbitmq_publisher(&self.topology).await?);
        self.metrics
            .cancellation_publisher_connected
            .store(true, Ordering::Relaxed);
        *guard = Some(publisher.clone());
        Ok(publisher)
    }

    pub fn spawn_rabbitmq_consumer(&self, state: AppState) -> Option<JoinHandle<()>> {
        if self.topology.mode != AsyncToolDispatchMode::RabbitMq {
            return None;
        }
        let topology = self.topology.clone();
        Some(tokio::spawn(async move {
            run_rabbitmq_consumer_loop(state, topology).await;
        }))
    }

    pub fn spawn_cancellation_consumer(&self, state: AppState) -> Option<JoinHandle<()>> {
        if self.topology.mode != AsyncToolDispatchMode::RabbitMq {
            return None;
        }
        let topology = self.topology.clone();
        Some(tokio::spawn(async move {
            run_cancellation_consumer_loop(state, topology).await;
        }))
    }

    pub async fn publish_cancellation(
        &self,
        invocation_id: &str,
    ) -> Result<(), AsyncToolEnqueueError> {
        if self.topology.mode != AsyncToolDispatchMode::RabbitMq {
            return Ok(());
        }
        let publisher = self.rabbitmq_publisher().await?;
        let payload = serde_json::to_vec(&InvocationCancellationEvent {
            invocation_id: invocation_id.to_string(),
        })
        .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
        let confirmation = publisher
            .channel
            .basic_publish(
                publisher.cancellation_exchange.as_str(),
                "",
                BasicPublishOptions {
                    mandatory: true,
                    ..BasicPublishOptions::default()
                },
                payload.as_slice(),
                BasicProperties::default()
                    .with_content_type("application/json".into())
                    .with_delivery_mode(2),
            )
            .await
            .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?
            .await
            .map_err(|error| AsyncToolEnqueueError::Unavailable(error.to_string()))?;
        match confirmation {
            Confirmation::Ack(None) => Ok(()),
            Confirmation::Ack(Some(_)) => {
                self.metrics
                    .cancellation_publisher_connected
                    .store(false, Ordering::Relaxed);
                Err(AsyncToolEnqueueError::Unavailable(
                    "RabbitMQ returned unroutable MCP cancellation event".to_string(),
                ))
            }
            Confirmation::Nack(_) => {
                self.metrics
                    .cancellation_publisher_connected
                    .store(false, Ordering::Relaxed);
                Err(AsyncToolEnqueueError::Unavailable(
                    "RabbitMQ rejected MCP cancellation event".to_string(),
                ))
            }
            Confirmation::NotRequested => {
                self.metrics
                    .cancellation_publisher_connected
                    .store(false, Ordering::Relaxed);
                Err(AsyncToolEnqueueError::Unavailable(
                    "RabbitMQ publisher confirm was not enabled for MCP cancellation event"
                        .to_string(),
                ))
            }
        }
    }
}
