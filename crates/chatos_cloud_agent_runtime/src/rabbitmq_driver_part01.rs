async fn ensure_topology(
    channel: &Channel,
    topology: &CloudAgentRabbitMqTopology,
) -> Result<(), String> {
    channel
        .exchange_declare(
            topology.exchange.as_str(),
            ExchangeKind::Direct,
            ExchangeDeclareOptions {
                durable: true,
                ..ExchangeDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_declare(
            topology.runtime_queue.as_str(),
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_bind(
            topology.runtime_queue.as_str(),
            topology.exchange.as_str(),
            topology.runtime_queue.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let mut retry_arguments = FieldTable::default();
    retry_arguments.insert(
        "x-dead-letter-exchange".into(),
        AMQPValue::LongString(topology.exchange.clone().into()),
    );
    retry_arguments.insert(
        "x-dead-letter-routing-key".into(),
        AMQPValue::LongString(topology.runtime_queue.clone().into()),
    );
    channel
        .queue_declare(
            topology.retry_queue.as_str(),
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            retry_arguments,
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_bind(
            topology.retry_queue.as_str(),
            topology.exchange.as_str(),
            topology.retry_queue.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    let dead_letter_queue = dead_letter_queue_name(topology);
    channel
        .queue_declare(
            dead_letter_queue.as_str(),
            QueueDeclareOptions {
                durable: true,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    channel
        .queue_bind(
            dead_letter_queue.as_str(),
            topology.exchange.as_str(),
            dead_letter_queue.as_str(),
            QueueBindOptions::default(),
            FieldTable::default(),
        )
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn dead_letter_queue_name(topology: &CloudAgentRabbitMqTopology) -> String {
    format!("{}.dead", topology.runtime_queue)
}

async fn defer_delivery(
    channel: &Channel,
    topology: &CloudAgentRabbitMqTopology,
    payload: &[u8],
    delivery_attempt: u32,
) -> Result<(), String> {
    let expiration = cloud_agent_retry_delay(topology.conflict_retry_delay, delivery_attempt)
        .as_millis()
        .max(1)
        .to_string();
    let confirmation = channel
        .basic_publish(
            topology.exchange.as_str(),
            topology.retry_queue.as_str(),
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload,
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_headers(cloud_agent_delivery_headers(delivery_attempt, None))
                .with_expiration(expiration.into()),
        )
        .await
        .map_err(|error| error.to_string())?
        .await
        .map_err(|error| error.to_string())?;
    confirmed("deferred Cloud Agent event", confirmation)
}

fn cloud_agent_retry_delay(base: Duration, delivery_attempt: u32) -> Duration {
    const MAX_RETRY_DELAY: Duration = Duration::from_secs(60);
    let exponent = delivery_attempt.saturating_sub(2).min(6);
    base.checked_mul(1_u32 << exponent)
        .unwrap_or(MAX_RETRY_DELAY)
        .min(MAX_RETRY_DELAY)
}

async fn dead_letter_delivery(
    channel: &Channel,
    topology: &CloudAgentRabbitMqTopology,
    payload: &[u8],
    delivery_attempt: u32,
    failure: &str,
) -> Result<(), String> {
    let queue = dead_letter_queue_name(topology);
    let confirmation = channel
        .basic_publish(
            topology.exchange.as_str(),
            queue.as_str(),
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload,
            BasicProperties::default()
                .with_content_type("application/json".into())
                .with_delivery_mode(2)
                .with_headers(cloud_agent_delivery_headers(
                    delivery_attempt,
                    Some(failure),
                )),
        )
        .await
        .map_err(|error| error.to_string())?
        .await
        .map_err(|error| error.to_string())?;
    confirmed("dead-lettered Cloud Agent event", confirmation)
}

async fn publish_intent(
    channel: &Channel,
    topology: &CloudAgentRabbitMqTopology,
    store: &CloudAgentStateStore,
    intent: &CloudAgentOutboxIntent,
) -> Result<(), String> {
    let routing_key = match intent.topic.as_str() {
        "ai_runtime_retry" => topology.retry_queue.as_str(),
        "mcp_tool_call_command" => intent.routing_key.as_str(),
        _ => topology.runtime_queue.as_str(),
    };
    let payload = if intent.topic == "mcp_tool_call_command" {
        let run = store
            .load_run(intent.ordering.agent_run_id.as_str())
            .await?
            .ok_or_else(|| "Cloud Agent run is missing while publishing MCP command".to_string())?;
        let session_ref = run
            .mcp_runtime_session_ref
            .as_deref()
            .ok_or_else(|| "Cloud Agent run has no MCP runtime session".to_string())?;
        serde_json::to_vec(&materialize_mcp_command(
            &run,
            intent,
            session_ref,
            topology.runtime_queue.as_str(),
        )?)
        .map_err(|error| error.to_string())?
    } else {
        serde_json::to_vec(intent).map_err(|error| error.to_string())?
    };
    let mut properties = BasicProperties::default()
        .with_content_type("application/json".into())
        .with_delivery_mode(2)
        .with_message_id(bounded_amqp_property_id(intent.event_id.as_str()).into())
        .with_correlation_id(bounded_amqp_property_id(intent.correlation_id.as_str()).into());
    if intent.topic == "ai_runtime_retry" {
        let delay = intent
            .available_at
            .signed_duration_since(chrono::Utc::now())
            .num_milliseconds()
            .max(1);
        properties = properties.with_expiration(delay.to_string().into());
    }
    let exchange = if intent.topic == "mcp_tool_call_command" {
        ""
    } else {
        topology.exchange.as_str()
    };
    let confirmation = channel
        .basic_publish(
            exchange,
            routing_key,
            BasicPublishOptions {
                mandatory: true,
                ..BasicPublishOptions::default()
            },
            payload.as_slice(),
            properties,
        )
        .await
        .map_err(|error| error.to_string())?
        .await
        .map_err(|error| error.to_string())?;
    confirmed(
        format!("Cloud Agent event for {routing_key}").as_str(),
        confirmation,
    )
}

fn bounded_amqp_property_id(value: &str) -> String {
    if value.len() <= MAX_AMQP_SHORT_STRING_BYTES {
        return value.to_string();
    }
    let digest = format!("{:x}", Sha256::digest(value.as_bytes()));
    let suffix = format!("#{digest}");
    let max_prefix_bytes = MAX_AMQP_SHORT_STRING_BYTES.saturating_sub(suffix.len());
    let mut prefix_end = max_prefix_bytes.min(value.len());
    while prefix_end > 0 && !value.is_char_boundary(prefix_end) {
        prefix_end -= 1;
    }
    format!("{}{suffix}", &value[..prefix_end])
}

fn outbox_publish_retry_delay(publish_attempt: u32) -> Duration {
    const MAX_RETRY_DELAY: Duration = Duration::from_secs(5 * 60);
    let exponent = publish_attempt.saturating_sub(1).min(9);
    Duration::from_secs(1_u64 << exponent).min(MAX_RETRY_DELAY)
}

fn outbox_reconcile_startup_jitter(interval: Duration, seed: &str) -> Duration {
    let max_jitter_ms = u64::try_from(interval.as_millis() / 4)
        .unwrap_or(u64::MAX)
        .min(1_000);
    if max_jitter_ms == 0 {
        return Duration::ZERO;
    }
    let digest = Sha256::digest(seed.as_bytes());
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    Duration::from_millis(u64::from_le_bytes(bytes) % (max_jitter_ms + 1))
}

fn confirmed(label: &str, confirmation: Confirmation) -> Result<(), String> {
    match confirmation {
        Confirmation::Ack(None) => Ok(()),
        Confirmation::Ack(Some(_)) => Err(format!("RabbitMQ returned unroutable {label}")),
        Confirmation::Nack(_) => Err(format!("RabbitMQ rejected {label}")),
        Confirmation::NotRequested => Err(format!(
            "RabbitMQ confirm mode is required while publishing {label}"
        )),
    }
}

#[cfg(test)]
#[path = "rabbitmq_driver/tests.rs"]
mod tests;
