// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use lapin::{
    options::{
        BasicConsumeOptions, BasicPublishOptions, BasicQosOptions, ConfirmSelectOptions,
        ExchangeDeclareOptions, QueueBindOptions, QueueDeclareOptions,
    },
    publisher_confirm::Confirmation,
    types::{AMQPValue, FieldTable},
    BasicProperties, Channel, Connection, ConnectionProperties, Consumer, ExchangeKind,
};

pub struct QueueTopology<'a> {
    pub exchange: &'a str,
    pub queue: &'a str,
    pub retry_queue: &'a str,
    pub dead_letter_queue: &'a str,
    pub retry_delay: Duration,
    pub stream_name: &'a str,
}

impl QueueTopology<'_> {
    fn validate(&self) -> Result<u32, String> {
        for (field, value) in [
            ("exchange", self.exchange),
            ("queue", self.queue),
            ("retry queue", self.retry_queue),
            ("dead-letter queue", self.dead_letter_queue),
        ] {
            if value.trim().is_empty() {
                return Err(format!(
                    "Memory Engine {} RabbitMQ {field} is empty",
                    self.stream_name
                ));
            }
        }
        if self.queue == self.retry_queue
            || self.queue == self.dead_letter_queue
            || self.retry_queue == self.dead_letter_queue
        {
            return Err(format!(
                "Memory Engine {} RabbitMQ queues must have distinct names",
                self.stream_name
            ));
        }
        u32::try_from(self.retry_delay.as_millis()).map_err(|_| {
            format!(
                "Memory Engine {} retry delay exceeds RabbitMQ limit",
                self.stream_name
            )
        })
    }
}

pub async fn open_publisher(
    rabbitmq_url: &str,
    topology: &QueueTopology<'_>,
) -> Result<(Connection, Channel), String> {
    let connection = Connection::connect(rabbitmq_url, ConnectionProperties::default())
        .await
        .map_err(|err| err.to_string())?;
    let channel = connection
        .create_channel()
        .await
        .map_err(|err| err.to_string())?;
    channel
        .confirm_select(ConfirmSelectOptions::default())
        .await
        .map_err(|err| err.to_string())?;
    ensure_topology(&channel, topology).await?;
    Ok((connection, channel))
}

pub async fn open_consumer(
    rabbitmq_url: &str,
    topology: &QueueTopology<'_>,
    consumer_tag: &str,
) -> Result<(Connection, Channel, Consumer), String> {
    let (connection, channel) = open_publisher(rabbitmq_url, topology).await?;
    channel
        .basic_qos(1, BasicQosOptions::default())
        .await
        .map_err(|err| err.to_string())?;
    let consumer = channel
        .basic_consume(
            topology.queue,
            consumer_tag,
            BasicConsumeOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok((connection, channel, consumer))
}

pub async fn publish_persistent_json(
    channel: &Channel,
    topology: &QueueTopology<'_>,
    routing_key: &str,
    payload: &[u8],
    message_id: String,
) -> Result<(), String> {
    let confirmation = channel
        .basic_publish(
            topology.exchange,
            routing_key,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload,
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_message_id(message_id.into()),
        )
        .await
        .map_err(|err| err.to_string())?
        .await
        .map_err(|err| err.to_string())?;
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!(
            "RabbitMQ returned unroutable Memory Engine {} event for {routing_key}",
            topology.stream_name
        )),
        Confirmation::Nack(_) => Err(format!(
            "RabbitMQ rejected Memory Engine {} event for {routing_key}",
            topology.stream_name
        )),
        Confirmation::NotRequested => Err(format!(
            "RabbitMQ publisher confirm was not enabled for Memory Engine {} event",
            topology.stream_name
        )),
    }
}

async fn ensure_topology(channel: &Channel, topology: &QueueTopology<'_>) -> Result<(), String> {
    let retry_delay_ms = topology.validate()?;
    channel
        .exchange_declare(
            topology.exchange,
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    declare_and_bind(
        channel,
        topology.exchange,
        topology.queue,
        FieldTable::default(),
    )
    .await?;

    let mut retry_arguments = FieldTable::default();
    retry_arguments.insert("x-message-ttl".into(), AMQPValue::LongUInt(retry_delay_ms));
    retry_arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(topology.exchange.into()),
    );
    retry_arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(topology.queue.into()),
    );
    declare_and_bind(
        channel,
        topology.exchange,
        topology.retry_queue,
        retry_arguments,
    )
    .await?;
    declare_and_bind(
        channel,
        topology.exchange,
        topology.dead_letter_queue,
        FieldTable::default(),
    )
    .await
}

async fn declare_and_bind(
    channel: &Channel,
    exchange: &str,
    queue: &str,
    arguments: FieldTable,
) -> Result<(), String> {
    channel
        .queue_declare(
            queue,
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            arguments,
        )
        .await
        .map_err(|err| err.to_string())?;
    channel
        .queue_bind(
            queue,
            exchange,
            queue,
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::QueueTopology;

    fn topology<'a>(queue: &'a str, retry: &'a str, dead: &'a str) -> QueueTopology<'a> {
        QueueTopology {
            exchange: "memory",
            queue,
            retry_queue: retry,
            dead_letter_queue: dead,
            retry_delay: Duration::from_secs(1),
            stream_name: "test",
        }
    }

    #[test]
    fn topology_requires_distinct_non_empty_queue_names() {
        assert!(topology("main", "retry", "dead").validate().is_ok());
        assert!(topology("main", "main", "dead").validate().is_err());
        assert!(topology("", "retry", "dead").validate().is_err());
    }

    #[test]
    fn topology_rejects_retry_ttl_larger_than_rabbitmq_supports() {
        let mut topology = topology("main", "retry", "dead");
        topology.retry_delay = Duration::from_millis(u64::from(u32::MAX) + 1);
        assert!(topology.validate().is_err());
    }
}
