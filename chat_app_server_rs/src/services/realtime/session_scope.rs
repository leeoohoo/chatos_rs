use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::core::validation::normalize_non_empty_str;
use crate::repositories::db::with_db;
use crate::services::memory_server_client;

use super::types::RealtimeEventEnvelope;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeTopicScope {
    Contacts,
    Notepad,
    Projects,
    Sessions,
    RemoteConnections,
    Conversation,
    Project,
    Terminal,
    RemoteConnection,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RealtimeTopic {
    pub scope: RealtimeTopicScope,
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RealtimeClientControlMessage {
    #[serde(rename = "type")]
    pub message_type: String,
    #[serde(default)]
    pub topics: Vec<RealtimeTopic>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RealtimeAckMessage {
    #[serde(rename = "type")]
    pub message_type: &'static str,
    pub acked: &'static str,
    pub topics: Vec<RealtimeTopic>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RealtimeErrorMessage {
    #[serde(rename = "type")]
    pub message_type: &'static str,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct RealtimeSubscriptionSet {
    topics: HashSet<RealtimeTopic>,
}

#[derive(Debug, Clone, Default)]
pub struct ConversationRealtimeScope {
    pub user_id: Option<String>,
}

pub async fn resolve_conversation_scope(
    conversation_id: &str,
) -> Result<ConversationRealtimeScope, String> {
    let conversation_id = normalize_non_empty_str(conversation_id)
        .ok_or_else(|| "conversation_id is required".to_string())?
        .to_string();

    let sqlite_lookup = with_db(
        |_db| Box::pin(async move { Ok(None::<ConversationRealtimeScope>) }),
        |pool| {
            let conversation_id = conversation_id.clone();
            Box::pin(async move {
                let row = sqlx::query_as::<_, SessionScopeRow>(
                    "SELECT id, user_id, project_id FROM sessions WHERE id = ? LIMIT 1",
                )
                .bind(&conversation_id)
                .fetch_optional(pool)
                .await
                .map_err(|err| err.to_string())?;
                Ok(row.map(|value| ConversationRealtimeScope {
                    user_id: normalize_non_empty_str(&value.user_id.unwrap_or_default())
                        .map(|value| value.to_string()),
                }))
            })
        },
    )
    .await?;

    if let Some(scope) = sqlite_lookup {
        return Ok(scope);
    }

    let session = memory_server_client::get_session_by_id(conversation_id.as_str())
        .await
        .map_err(|err| {
            format!(
                "load conversation {} from memory server failed: {}",
                conversation_id, err
            )
        })?;

    Ok(ConversationRealtimeScope {
        user_id: session
            .as_ref()
            .and_then(|value| value.user_id.as_deref())
            .and_then(normalize_non_empty_str)
            .map(|value| value.to_string()),
    })
}

impl RealtimeSubscriptionSet {
    pub fn subscribe(&mut self, topics: Vec<RealtimeTopic>) -> Result<Vec<RealtimeTopic>, String> {
        let normalized = normalize_topics(topics)?;
        for topic in &normalized {
            self.topics.insert(topic.clone());
        }
        Ok(normalized)
    }

    pub fn unsubscribe(&mut self, topics: Vec<RealtimeTopic>) -> Result<Vec<RealtimeTopic>, String> {
        let normalized = normalize_topics(topics)?;
        for topic in &normalized {
            self.topics.remove(topic);
        }
        Ok(normalized)
    }

    pub fn allows(&self, envelope: &RealtimeEventEnvelope) -> bool {
        if self.topics.is_empty() {
            return false;
        }

        let envelope_topics = topics_for_envelope(envelope);
        envelope_topics.into_iter().any(|topic| self.topics.contains(&topic))
    }
}

fn normalize_topics(topics: Vec<RealtimeTopic>) -> Result<Vec<RealtimeTopic>, String> {
    let mut normalized = Vec::with_capacity(topics.len());
    let mut seen = HashSet::new();
    for topic in topics {
        let normalized_topic = normalize_topic(topic)?;
        if seen.insert(normalized_topic.clone()) {
            normalized.push(normalized_topic);
        }
    }
    Ok(normalized)
}

fn normalize_topic(topic: RealtimeTopic) -> Result<RealtimeTopic, String> {
    let normalized_id = topic
        .id
        .as_deref()
        .and_then(normalize_non_empty_str)
        .map(|value| value.to_string());
    let requires_id = matches!(
        topic.scope,
        RealtimeTopicScope::Conversation
            | RealtimeTopicScope::Project
            | RealtimeTopicScope::Terminal
            | RealtimeTopicScope::RemoteConnection
    );
    if requires_id && normalized_id.is_none() {
        return Err(format!("topic {:?} requires id", topic.scope));
    }
    Ok(RealtimeTopic {
        scope: topic.scope,
        id: normalized_id,
    })
}

fn topics_for_envelope(envelope: &RealtimeEventEnvelope) -> Vec<RealtimeTopic> {
    let mut topics = Vec::new();
    let event = envelope.event;

    if event == "contacts.updated" {
        topics.push(RealtimeTopic {
            scope: RealtimeTopicScope::Contacts,
            id: None,
        });
    }
    if event == "notepad.updated" {
        topics.push(RealtimeTopic {
            scope: RealtimeTopicScope::Notepad,
            id: None,
        });
    }
    if event == "projects.updated" {
        topics.push(RealtimeTopic {
            scope: RealtimeTopicScope::Projects,
            id: None,
        });
    }
    if event == "sessions.updated" {
        topics.push(RealtimeTopic {
            scope: RealtimeTopicScope::Sessions,
            id: None,
        });
    }
    if event == "remote_connections.updated" {
        topics.push(RealtimeTopic {
            scope: RealtimeTopicScope::RemoteConnections,
            id: None,
        });
    }

    if let Some(conversation_id) = envelope
        .conversation_id
        .as_deref()
        .and_then(normalize_non_empty_str)
    {
        topics.push(RealtimeTopic {
            scope: RealtimeTopicScope::Conversation,
            id: Some(conversation_id.to_string()),
        });
    }

    if let Some(project_id) = envelope.project_id.as_deref().and_then(normalize_non_empty_str) {
        topics.push(RealtimeTopic {
            scope: RealtimeTopicScope::Project,
            id: Some(project_id.to_string()),
        });
    }

    if let Some(terminal_id) = extract_string_path(envelope, &["payload", "terminal_id"]) {
        topics.push(RealtimeTopic {
            scope: RealtimeTopicScope::Terminal,
            id: Some(terminal_id),
        });
    }

    if let Some(connection_id) = extract_string_path(envelope, &["payload", "connection_id"]) {
        topics.push(RealtimeTopic {
            scope: RealtimeTopicScope::RemoteConnection,
            id: Some(connection_id),
        });
    }

    topics
}

fn extract_string_path(envelope: &RealtimeEventEnvelope, path: &[&str]) -> Option<String> {
    let value = serde_json::to_value(envelope).ok()?;
    let mut current = &value;
    for key in path {
        current = current.get(*key)?;
    }
    current
        .as_str()
        .and_then(normalize_non_empty_str)
        .map(|value| value.to_string())
}

#[derive(sqlx::FromRow)]
struct SessionScopeRow {
    user_id: Option<String>,
}
