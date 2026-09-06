// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DevicePresence {
    pub instance_id: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelaySessionIdentity {
    pub owner_user_id: String,
    pub device_id: String,
    pub session_id: String,
}

impl DevicePresence {
    pub fn relay_identity(&self) -> RelaySessionIdentity {
        RelaySessionIdentity {
            owner_user_id: self.owner_user_id.clone(),
            device_id: self.device_id.clone(),
            session_id: self.session_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelayCorrelation {
    pub requester_instance_id: String,
    pub source: RelaySessionIdentity,
}

#[derive(Clone)]
pub struct ValkeyCoordinator {
    client: redis::Client,
    connection: ConnectionManager,
    key_prefix: String,
    device_presence_ttl: Duration,
    terminal_subscriber_ttl: Duration,
}

impl ValkeyCoordinator {
    pub async fn connect(
        valkey_url: &str,
        key_prefix: &str,
        device_presence_ttl: Duration,
        terminal_subscriber_ttl: Duration,
    ) -> Result<Self, String> {
        let client = redis::Client::open(valkey_url)
            .map_err(|error| format!("parse Local Connector Valkey URL failed: {error}"))?;
        let connection = client
            .get_connection_manager()
            .await
            .map_err(|error| format!("connect Local Connector Valkey failed: {error}"))?;
        Ok(Self {
            client,
            connection,
            key_prefix: key_prefix.trim_end_matches(':').to_string(),
            device_presence_ttl,
            terminal_subscriber_ttl,
        })
    }

    pub async fn consume_device_nonce(
        &self,
        device_id: &str,
        nonce: &str,
        retention: Duration,
    ) -> Result<bool, String> {
        let digest = Sha256::digest(format!("{device_id}\0{nonce}").as_bytes());
        let key = format!("{}:device-nonce:{}", self.key_prefix, hex::encode(digest));
        let ttl_seconds = retention.as_secs().saturating_mul(2).max(1);
        let mut connection = self.connection.clone();
        let result: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg("1")
            .arg("NX")
            .arg("EX")
            .arg(ttl_seconds)
            .query_async(&mut connection)
            .await
            .map_err(|error| format!("consume Local Connector device nonce failed: {error}"))?;
        Ok(result.is_some())
    }

    pub async fn register_device_presence(&self, presence: &DevicePresence) -> Result<(), String> {
        let key = self.device_presence_key(presence.device_id.as_str());
        let value = serde_json::to_string(presence).map_err(|error| error.to_string())?;
        let mut connection = self.connection.clone();
        redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("EX")
            .arg(self.device_presence_ttl.as_secs().max(1))
            .query_async::<()>(&mut connection)
            .await
            .map_err(|error| format!("register Local Connector device presence failed: {error}"))
    }

    pub async fn refresh_device_presence(&self, presence: &DevicePresence) -> Result<bool, String> {
        self.compare_and_expire_or_delete(presence, true).await
    }

    pub async fn unregister_device_presence(
        &self,
        presence: &DevicePresence,
    ) -> Result<bool, String> {
        self.compare_and_expire_or_delete(presence, false).await
    }

    pub async fn device_presence(&self, device_id: &str) -> Result<Option<DevicePresence>, String> {
        let mut connection = self.connection.clone();
        let value: Option<String> = redis::cmd("GET")
            .arg(self.device_presence_key(device_id))
            .query_async(&mut connection)
            .await
            .map_err(|error| format!("load Local Connector device presence failed: {error}"))?;
        value
            .map(|value| {
                serde_json::from_str(&value).map_err(|error| {
                    format!("parse Local Connector device presence failed: {error}")
                })
            })
            .transpose()
    }

    pub async fn register_relay_correlation(
        &self,
        request_id: &str,
        correlation: &RelayCorrelation,
        ttl: Duration,
    ) -> Result<bool, String> {
        let value = serde_json::to_string(correlation).map_err(|error| error.to_string())?;
        let mut connection = self.connection.clone();
        let result: Option<String> = redis::cmd("SET")
            .arg(self.relay_correlation_key(request_id))
            .arg(value)
            .arg("NX")
            .arg("EX")
            .arg(ttl.as_secs().max(1))
            .query_async(&mut connection)
            .await
            .map_err(|error| {
                format!("register Local Connector relay correlation failed: {error}")
            })?;
        Ok(result.is_some())
    }

    pub async fn relay_correlation(
        &self,
        request_id: &str,
    ) -> Result<Option<RelayCorrelation>, String> {
        let mut connection = self.connection.clone();
        let value: Option<String> = redis::cmd("GET")
            .arg(self.relay_correlation_key(request_id))
            .query_async(&mut connection)
            .await
            .map_err(|error| format!("load Local Connector relay correlation failed: {error}"))?;
        value
            .map(|value| {
                serde_json::from_str(&value).map_err(|error| {
                    format!("parse Local Connector relay correlation failed: {error}")
                })
            })
            .transpose()
    }

    pub async fn delete_relay_correlation(
        &self,
        request_id: &str,
        requester_instance_id: &str,
    ) -> Result<bool, String> {
        let script = redis::Script::new(
            "local value = redis.call('GET', KEYS[1]); if not value then return 0 end; local decoded = cjson.decode(value); if decoded.requester_instance_id == ARGV[1] then return redis.call('DEL', KEYS[1]) else return 0 end",
        );
        let mut connection = self.connection.clone();
        let deleted: i64 = script
            .key(self.relay_correlation_key(request_id))
            .arg(requester_instance_id)
            .invoke_async(&mut connection)
            .await
            .map_err(|error| format!("delete Local Connector relay correlation failed: {error}"))?;
        Ok(deleted == 1)
    }

    pub async fn publish_instance_message<T: Serialize>(
        &self,
        instance_id: &str,
        message: &T,
    ) -> Result<(), String> {
        let payload = serde_json::to_string(message).map_err(|error| error.to_string())?;
        let mut connection = self.connection.clone();
        let subscribers: i64 = redis::cmd("PUBLISH")
            .arg(self.instance_channel(instance_id))
            .arg(payload)
            .query_async(&mut connection)
            .await
            .map_err(|error| format!("publish Local Connector instance message failed: {error}"))?;
        if subscribers == 0 {
            return Err(format!(
                "Local Connector target instance {instance_id} has no active control subscriber"
            ));
        }
        Ok(())
    }

    pub async fn subscribe_instance(
        &self,
        instance_id: &str,
    ) -> Result<redis::aio::PubSub, String> {
        let mut pubsub =
            self.client.get_async_pubsub().await.map_err(|error| {
                format!("connect Local Connector Valkey PubSub failed: {error}")
            })?;
        pubsub
            .subscribe(self.instance_channel(instance_id))
            .await
            .map_err(|error| {
                format!("subscribe Local Connector instance channel failed: {error}")
            })?;
        Ok(pubsub)
    }

    pub async fn register_terminal_subscriber(
        &self,
        terminal_session_id: &str,
        instance_id: &str,
    ) -> Result<(), String> {
        let expires_at =
            unix_timestamp_seconds().saturating_add(self.terminal_subscriber_ttl.as_secs().max(1));
        let key_ttl = self
            .terminal_subscriber_ttl
            .as_secs()
            .saturating_mul(2)
            .max(1);
        let script = redis::Script::new(
            "redis.call('ZADD', KEYS[1], ARGV[1], ARGV[2]); redis.call('EXPIRE', KEYS[1], ARGV[3]); return 1",
        );
        let mut connection = self.connection.clone();
        script
            .key(self.terminal_subscribers_key(terminal_session_id))
            .arg(expires_at)
            .arg(instance_id)
            .arg(key_ttl)
            .invoke_async::<i64>(&mut connection)
            .await
            .map_err(|error| {
                format!("register Local Connector terminal subscriber failed: {error}")
            })?;
        Ok(())
    }

    pub async fn unregister_terminal_subscriber(
        &self,
        terminal_session_id: &str,
        instance_id: &str,
    ) -> Result<(), String> {
        let mut connection = self.connection.clone();
        redis::cmd("ZREM")
            .arg(self.terminal_subscribers_key(terminal_session_id))
            .arg(instance_id)
            .query_async::<()>(&mut connection)
            .await
            .map_err(|error| {
                format!("unregister Local Connector terminal subscriber failed: {error}")
            })
    }

    pub async fn terminal_subscriber_instances(
        &self,
        terminal_session_id: &str,
    ) -> Result<Vec<String>, String> {
        let now = unix_timestamp_seconds();
        let script = redis::Script::new(
            "redis.call('ZREMRANGEBYSCORE', KEYS[1], '-inf', ARGV[1]); return redis.call('ZRANGEBYSCORE', KEYS[1], ARGV[2], '+inf')",
        );
        let mut connection = self.connection.clone();
        script
            .key(self.terminal_subscribers_key(terminal_session_id))
            .arg(now)
            .arg(format!("({now}"))
            .invoke_async(&mut connection)
            .await
            .map_err(|error| format!("load Local Connector terminal subscribers failed: {error}"))
    }

    pub async fn register_terminal_session_binding(
        &self,
        terminal_session_id: &str,
        source: &RelaySessionIdentity,
    ) -> Result<bool, String> {
        let value = serde_json::to_string(source).map_err(|error| error.to_string())?;
        let ttl = self
            .terminal_subscriber_ttl
            .as_secs()
            .saturating_mul(2)
            .max(1);
        let script = redis::Script::new(
            "local current = redis.call('GET', KEYS[1]); if current and current ~= ARGV[1] then return 0 end; redis.call('SET', KEYS[1], ARGV[1], 'EX', ARGV[2]); return 1",
        );
        let mut connection = self.connection.clone();
        let registered: i64 = script
            .key(self.terminal_session_binding_key(terminal_session_id))
            .arg(value)
            .arg(ttl)
            .invoke_async(&mut connection)
            .await
            .map_err(|error| {
                format!("register Local Connector terminal session binding failed: {error}")
            })?;
        Ok(registered == 1)
    }

    pub async fn terminal_session_binding(
        &self,
        terminal_session_id: &str,
    ) -> Result<Option<RelaySessionIdentity>, String> {
        let mut connection = self.connection.clone();
        let value: Option<String> = redis::cmd("GET")
            .arg(self.terminal_session_binding_key(terminal_session_id))
            .query_async(&mut connection)
            .await
            .map_err(|error| {
                format!("load Local Connector terminal session binding failed: {error}")
            })?;
        value
            .map(|value| {
                serde_json::from_str(&value).map_err(|error| {
                    format!("parse Local Connector terminal session binding failed: {error}")
                })
            })
            .transpose()
    }

    async fn compare_and_expire_or_delete(
        &self,
        presence: &DevicePresence,
        refresh: bool,
    ) -> Result<bool, String> {
        let key = self.device_presence_key(presence.device_id.as_str());
        let value = serde_json::to_string(presence).map_err(|error| error.to_string())?;
        let script = if refresh {
            redis::Script::new(
                "if redis.call('GET', KEYS[1]) == ARGV[1] then return redis.call('EXPIRE', KEYS[1], ARGV[2]) else return 0 end",
            )
        } else {
            redis::Script::new(
                "if redis.call('GET', KEYS[1]) == ARGV[1] then return redis.call('DEL', KEYS[1]) else return 0 end",
            )
        };
        let mut invocation = script.prepare_invoke();
        invocation.key(key).arg(value);
        if refresh {
            invocation.arg(self.device_presence_ttl.as_secs().max(1));
        }
        let mut connection = self.connection.clone();
        let changed: i64 = invocation
            .invoke_async(&mut connection)
            .await
            .map_err(|error| format!("update Local Connector device presence failed: {error}"))?;
        Ok(changed == 1)
    }

    fn device_presence_key(&self, device_id: &str) -> String {
        format!("{}:device-presence:{device_id}", self.key_prefix)
    }

    fn relay_correlation_key(&self, request_id: &str) -> String {
        format!("{}:relay-correlation:{request_id}", self.key_prefix)
    }

    fn instance_channel(&self, instance_id: &str) -> String {
        format!("{}:instance:{instance_id}", self.key_prefix)
    }

    fn terminal_subscribers_key(&self, terminal_session_id: &str) -> String {
        let digest = Sha256::digest(terminal_session_id.as_bytes());
        format!(
            "{}:terminal-subscribers:{}",
            self.key_prefix,
            hex::encode(digest)
        )
    }

    fn terminal_session_binding_key(&self, terminal_session_id: &str) -> String {
        let digest = Sha256::digest(terminal_session_id.as_bytes());
        format!(
            "{}:terminal-session-binding:{}",
            self.key_prefix,
            hex::encode(digest)
        )
    }
}

fn unix_timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::{DevicePresence, RelayCorrelation};

    #[test]
    fn device_presence_contains_routing_identity_without_socket_state() {
        let presence = DevicePresence {
            instance_id: "local-connector-1".to_string(),
            owner_user_id: "owner-1".to_string(),
            device_id: "device-1".to_string(),
            session_id: "session-1".to_string(),
        };
        let value = serde_json::to_value(&presence).expect("serialize presence");
        assert_eq!(value["instance_id"], "local-connector-1");
        assert!(value.get("outbound").is_none());
    }

    #[test]
    fn relay_correlation_contains_only_request_routing_metadata() {
        let correlation = RelayCorrelation {
            requester_instance_id: "local-connector-2".to_string(),
            source: super::RelaySessionIdentity {
                owner_user_id: "owner-1".to_string(),
                device_id: "device-1".to_string(),
                session_id: "session-1".to_string(),
            },
        };
        let value = serde_json::to_value(&correlation).expect("serialize correlation");
        assert_eq!(value["requester_instance_id"], "local-connector-2");
        assert_eq!(value["source"]["owner_user_id"], "owner-1");
        assert_eq!(value["source"]["device_id"], "device-1");
        assert_eq!(value["source"]["session_id"], "session-1");
        assert!(value.get("response").is_none());
    }
}
