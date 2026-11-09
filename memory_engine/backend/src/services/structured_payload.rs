// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub(crate) const MEMORY_PAYLOAD_SCHEMA: &str = "memory_payload_v1";

const FACT_COLLECTIONS: [&str; 8] = [
    "completed",
    "decisions",
    "open_items",
    "blockers",
    "artifacts",
    "constraints",
    "stable_preferences",
    "uncertainties",
];

#[cfg(test)]
pub(crate) struct GeneratedPayloadScope<'a> {
    pub kind: &'a str,
    pub subject_id: &'a str,
    pub project_partition_key: Option<&'a str>,
}

#[cfg(test)]
pub(crate) fn build_generated_payload(
    generation_id: &str,
    scope: GeneratedPayloadScope<'_>,
    generated_text: &str,
    source_refs: &[String],
    observed_at: Option<&str>,
) -> Result<serde_json::Value, String> {
    let text = generated_text.trim();
    if text.is_empty() {
        return Err("generated memory payload text is empty".to_string());
    }
    if source_refs.is_empty() || source_refs.iter().any(|value| value.trim().is_empty()) {
        return Err("generated memory payload source refs are incomplete".to_string());
    }
    let fact_key = format!("memory.{}.context", scope.kind);
    let fact_id = deterministic_fact_id(
        generation_id,
        "open_items",
        fact_key.as_str(),
        &serde_json::Value::String(text.to_string()),
        source_refs,
    )?;
    Ok(serde_json::json!({
        "schema_version": MEMORY_PAYLOAD_SCHEMA,
        "scope": {
            "kind": scope.kind,
            "subject_id": scope.subject_id,
            "project_partition_key": scope.project_partition_key,
        },
        "current_goal": text,
        "completed": [],
        "decisions": [],
        "open_items": [{
            "fact_id": fact_id,
            "fact_key": fact_key,
            "value": text,
            "status": "active",
            "supersedes": [],
            "source_refs": source_refs,
            "observed_at": observed_at,
            "confidence": "model_inference",
        }],
        "blockers": [],
        "artifacts": [],
        "constraints": [],
        "stable_preferences": [],
        "uncertainties": [],
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct MemoryPayloadContract {
    pub generation_id: String,
    pub scope_kind: String,
    pub subject_id: String,
    pub project_partition_key: Option<String>,
    pub source_refs: Vec<String>,
    pub supersession_candidates: Vec<String>,
}

impl MemoryPayloadContract {
    pub(crate) fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("generation_id", self.generation_id.as_str()),
            ("scope_kind", self.scope_kind.as_str()),
            ("subject_id", self.subject_id.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("memory payload contract {name} is empty"));
            }
        }
        if !matches!(self.scope_kind.as_str(), "thread" | "project" | "agent") {
            return Err("memory payload contract scope kind is unsupported".to_string());
        }
        match (
            self.scope_kind.as_str(),
            self.project_partition_key.as_deref(),
        ) {
            ("agent", None) => {}
            ("agent", Some(_)) => {
                return Err(
                    "agent memory payload contract cannot have a project partition".to_string(),
                );
            }
            ("project", Some(partition)) if valid_project_partition(partition) => {}
            ("project", _) => {
                return Err(
                    "project memory payload contract requires a project partition".to_string(),
                );
            }
            ("thread", Some(partition)) if valid_thread_partition(partition) => {}
            ("thread", _) => {
                return Err(
                    "thread memory payload contract requires a valid frozen partition".to_string(),
                );
            }
            _ => unreachable!("scope kind was validated above"),
        }
        let refs = self
            .source_refs
            .iter()
            .map(|value| value.trim())
            .collect::<HashSet<_>>();
        if refs.len() != self.source_refs.len() || refs.contains("") || refs.is_empty() {
            return Err(
                "memory payload contract source refs must be unique and non-empty".to_string(),
            );
        }
        let candidates = self
            .supersession_candidates
            .iter()
            .map(|value| value.trim())
            .collect::<HashSet<_>>();
        if candidates.len() != self.supersession_candidates.len() || candidates.contains("") {
            return Err(
                "memory payload contract supersession candidates must be unique and non-empty"
                    .to_string(),
            );
        }
        Ok(())
    }

    pub(crate) fn response_format(&self) -> Result<serde_json::Value, String> {
        self.validate()?;
        let fact = serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "fact_id", "fact_key", "value", "status", "supersedes", "source_refs",
                "observed_at", "confidence"
            ],
            "properties": {
                "fact_id": {"type":"string", "minLength":1, "maxLength":256},
                "fact_key": {
                    "type":"string",
                    "pattern":"^[A-Za-z0-9._:/-]+$",
                    "minLength":1,
                    "maxLength":256
                },
                "value": {"type":"string", "minLength":1, "maxLength":16384},
                "status": {"type":"string", "enum":["active", "superseded", "uncertain"]},
                "supersedes": {
                    "type":"array",
                    "uniqueItems":true,
                    "items": {"type":"string", "enum":self.supersession_candidates}
                },
                "source_refs": {
                    "type":"array",
                    "minItems":1,
                    "uniqueItems":true,
                    "items": {"type":"string", "enum":self.source_refs}
                },
                "observed_at": {"type":["string", "null"], "format":"date-time"},
                "confidence": {
                    "type":"string",
                    "enum":["explicit_user", "tool_evidence", "model_inference", "uncertain"]
                }
            }
        });
        let mut properties = serde_json::Map::new();
        properties.insert(
            "schema_version".to_string(),
            serde_json::json!({"type":"string", "const":MEMORY_PAYLOAD_SCHEMA}),
        );
        properties.insert(
            "scope".to_string(),
            serde_json::json!({
                "type":"object",
                "additionalProperties":false,
                "required":["kind", "subject_id", "project_partition_key"],
                "properties":{
                    "kind":{"type":"string", "const":self.scope_kind},
                    "subject_id":{"type":"string", "const":self.subject_id},
                    "project_partition_key": match &self.project_partition_key {
                        Some(value) => serde_json::json!({"type":"string", "const":value}),
                        None => serde_json::json!({"type":"null"}),
                    }
                }
            }),
        );
        properties.insert(
            "current_goal".to_string(),
            serde_json::json!({"type":["string", "null"], "maxLength":16384}),
        );
        for collection in FACT_COLLECTIONS {
            properties.insert(
                collection.to_string(),
                serde_json::json!({"type":"array", "items":fact}),
            );
        }
        Ok(serde_json::json!({
            "type":"json_schema",
            "name":MEMORY_PAYLOAD_SCHEMA,
            "strict":true,
            "schema":{
                "type":"object",
                "additionalProperties":false,
                "required":[
                    "schema_version", "scope", "current_goal", "completed", "decisions",
                    "open_items", "blockers", "artifacts", "constraints",
                    "stable_preferences", "uncertainties"
                ],
                "properties":properties
            }
        }))
    }

    pub(crate) fn model_instruction(&self) -> Result<String, String> {
        self.validate()?;
        Ok(format!(
            "Return exactly one {schema} JSON object matching the enforced response schema. Classify each durable statement into the narrowest fact collection. Every fact must cite only source_refs visible in the supplied source items. Use supersedes only when a newer fact directly replaces one of these frozen candidate IDs: {candidates}. fact_id is a required model-local identifier and will be replaced by the server's deterministic identifier. Do not include credentials, authorization grants, or facts unsupported by the sources.",
            schema = MEMORY_PAYLOAD_SCHEMA,
            candidates = serde_json::to_string(&self.supersession_candidates)
                .map_err(|error| error.to_string())?,
        ))
    }

    pub(crate) fn finalize_model_output(
        &self,
        raw_output: &str,
    ) -> Result<serde_json::Value, String> {
        self.validate()?;
        let mut payload: serde_json::Value = serde_json::from_str(raw_output.trim())
            .map_err(|error| format!("decode model memory_payload_v1 failed: {error}"))?;
        validate_envelope_shape(&payload)?;
        let scope = payload
            .get("scope")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| "memory payload scope must be an object".to_string())?;
        let payload_partition = match scope.get("project_partition_key") {
            Some(value) if value.is_null() => None,
            Some(value) => Some(value.as_str().ok_or_else(|| {
                "model memory payload project partition must be a string or null".to_string()
            })?),
            None => {
                return Err("model memory payload project partition is missing".to_string());
            }
        };
        if scope.get("kind").and_then(serde_json::Value::as_str) != Some(self.scope_kind.as_str())
            || scope.get("subject_id").and_then(serde_json::Value::as_str)
                != Some(self.subject_id.as_str())
            || payload_partition != self.project_partition_key.as_deref()
        {
            return Err("model memory payload scope does not match frozen generation".to_string());
        }
        let allowed_refs = self.source_refs.iter().collect::<HashSet<_>>();
        let allowed_supersedes = self.supersession_candidates.iter().collect::<HashSet<_>>();
        let mut fact_count = 0usize;
        let mut deterministic_ids = HashSet::new();
        for collection in FACT_COLLECTIONS {
            let facts = payload
                .get_mut(collection)
                .and_then(serde_json::Value::as_array_mut)
                .ok_or_else(|| format!("memory payload {collection} must be an array"))?;
            for fact in facts {
                let object = fact
                    .as_object_mut()
                    .ok_or_else(|| format!("memory payload {collection} fact must be an object"))?;
                validate_exact_keys(
                    object,
                    &[
                        "fact_id",
                        "fact_key",
                        "value",
                        "status",
                        "supersedes",
                        "source_refs",
                        "observed_at",
                        "confidence",
                    ],
                    format!("memory payload {collection} fact").as_str(),
                )?;
                let model_fact_id = required_model_text(object.get("fact_id"), "fact_id")?;
                if model_fact_id.chars().count() > 256 {
                    return Err("model memory fact fact_id exceeds 256 characters".to_string());
                }
                let fact_key = required_model_text(object.get("fact_key"), "fact_key")?;
                if fact_key.chars().count() > 256
                    || !fact_key.chars().all(|character| {
                        character.is_ascii_alphanumeric() || "._:/-".contains(character)
                    })
                {
                    return Err("model memory fact fact_key is invalid".to_string());
                }
                let value = object
                    .get("value")
                    .filter(|value| value.as_str().is_some_and(|value| !value.trim().is_empty()))
                    .ok_or_else(|| "model memory fact value is missing".to_string())?;
                if value
                    .as_str()
                    .is_some_and(|value| value.chars().count() > 16_384)
                {
                    return Err("model memory fact value exceeds 16384 characters".to_string());
                }
                let status = required_model_text(object.get("status"), "status")?;
                if !matches!(status, "active" | "superseded" | "uncertain") {
                    return Err("model memory fact status is invalid".to_string());
                }
                let source_refs = model_string_array(object.get("source_refs"), "source_refs")?;
                if source_refs.is_empty()
                    || source_refs
                        .iter()
                        .any(|value| !allowed_refs.contains(value))
                {
                    return Err(
                        "model memory fact source_refs are outside the frozen input manifest"
                            .to_string(),
                    );
                }
                let supersedes = model_string_array(object.get("supersedes"), "supersedes")?;
                if supersedes
                    .iter()
                    .any(|value| !allowed_supersedes.contains(value))
                {
                    return Err(
                        "model memory fact supersedes is outside the frozen candidates".to_string(),
                    );
                }
                match object.get("observed_at") {
                    Some(value) if value.is_null() => {}
                    Some(value) => {
                        let observed_at = value.as_str().ok_or_else(|| {
                            "model memory fact observed_at must be a timestamp or null".to_string()
                        })?;
                        chrono::DateTime::parse_from_rfc3339(observed_at).map_err(|_| {
                            "model memory fact observed_at must be RFC3339".to_string()
                        })?;
                    }
                    None => {
                        return Err("model memory fact observed_at is missing".to_string());
                    }
                }
                let confidence = required_model_text(object.get("confidence"), "confidence")?;
                if !matches!(
                    confidence,
                    "explicit_user" | "tool_evidence" | "model_inference" | "uncertain"
                ) {
                    return Err("model memory fact confidence is invalid".to_string());
                }
                let fact_id = deterministic_fact_id(
                    self.generation_id.as_str(),
                    collection,
                    fact_key,
                    value,
                    &source_refs,
                )?;
                if !deterministic_ids.insert(fact_id.clone()) {
                    return Err("model memory payload contains duplicate durable facts".to_string());
                }
                object.insert("fact_id".to_string(), serde_json::Value::String(fact_id));
                fact_count = fact_count.saturating_add(1);
            }
        }
        if fact_count == 0 {
            return Err("model memory payload must contain at least one fact".to_string());
        }
        Ok(payload)
    }
}

pub(crate) fn render_payload(payload: &serde_json::Value) -> Result<String, String> {
    validate_envelope_shape(payload)?;
    let mut rendered = Vec::new();
    if let Some(goal) = payload
        .get("current_goal")
        .and_then(serde_json::Value::as_str)
    {
        let goal = goal.trim();
        if !goal.is_empty() {
            rendered.push(format!("current_goal: {goal}"));
        }
    }
    for collection in FACT_COLLECTIONS {
        let facts = payload
            .get(collection)
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("memory payload {collection} must be an array"))?;
        for fact in facts {
            let key = fact
                .get("fact_key")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("memory payload {collection} fact_key is missing"))?;
            let value = fact
                .get("value")
                .ok_or_else(|| format!("memory payload {collection} value is missing"))?;
            rendered.push(format!("{key}: {}", render_value(value)?));
        }
    }
    if rendered.is_empty() {
        return Err("memory payload has no renderable content".to_string());
    }
    Ok(rendered.join("\n"))
}

fn required_model_text<'a>(
    value: Option<&'a serde_json::Value>,
    name: &str,
) -> Result<&'a str, String> {
    value
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("model memory fact {name} is missing"))
}

fn model_string_array(
    value: Option<&serde_json::Value>,
    name: &str,
) -> Result<Vec<String>, String> {
    let values = value
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("model memory fact {name} must be an array"))?;
    let result = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("model memory fact {name} contains an invalid id"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if result.iter().collect::<HashSet<_>>().len() != result.len() {
        return Err(format!("model memory fact {name} contains duplicate ids"));
    }
    Ok(result)
}

pub(crate) fn validate_envelope_shape(payload: &serde_json::Value) -> Result<(), String> {
    let object = payload
        .as_object()
        .ok_or_else(|| "memory payload must be an object".to_string())?;
    validate_exact_keys(
        object,
        &[
            "schema_version",
            "scope",
            "current_goal",
            "completed",
            "decisions",
            "open_items",
            "blockers",
            "artifacts",
            "constraints",
            "stable_preferences",
            "uncertainties",
        ],
        "memory payload",
    )?;
    if object
        .get("schema_version")
        .and_then(serde_json::Value::as_str)
        != Some(MEMORY_PAYLOAD_SCHEMA)
    {
        return Err("memory payload schema_version is not memory_payload_v1".to_string());
    }
    if !object
        .get("scope")
        .is_some_and(serde_json::Value::is_object)
    {
        return Err("memory payload scope must be an object".to_string());
    }
    validate_exact_keys(
        object
            .get("scope")
            .and_then(serde_json::Value::as_object)
            .expect("scope object was checked above"),
        &["kind", "subject_id", "project_partition_key"],
        "memory payload scope",
    )?;
    match object.get("current_goal") {
        Some(value)
            if value.is_null()
                || value
                    .as_str()
                    .is_some_and(|value| value.chars().count() <= 16_384) => {}
        _ => return Err("memory payload current_goal must be a string or null".to_string()),
    }
    for collection in FACT_COLLECTIONS {
        if !object
            .get(collection)
            .is_some_and(serde_json::Value::is_array)
        {
            return Err(format!("memory payload {collection} must be an array"));
        }
    }
    Ok(())
}

fn validate_exact_keys(
    object: &serde_json::Map<String, serde_json::Value>,
    expected: &[&str],
    label: &str,
) -> Result<(), String> {
    let expected = expected.iter().copied().collect::<HashSet<_>>();
    let actual = object.keys().map(String::as_str).collect::<HashSet<_>>();
    if actual != expected {
        return Err(format!("{label} fields do not match the enforced schema"));
    }
    Ok(())
}

fn valid_thread_partition(value: &str) -> bool {
    value == "__none__" || valid_project_partition(value)
}

fn valid_project_partition(value: &str) -> bool {
    value
        .strip_prefix("project:")
        .is_some_and(|project_id| !project_id.trim().is_empty() && project_id == project_id.trim())
}

pub(crate) fn deterministic_fact_id(
    generation_id: &str,
    collection: &str,
    fact_key: &str,
    value: &serde_json::Value,
    source_refs: &[String],
) -> Result<String, String> {
    let value = serde_json::to_string(value).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    for component in [generation_id, collection, fact_key, value.as_str()] {
        hasher.update((component.len() as u64).to_be_bytes());
        hasher.update(component.as_bytes());
    }
    for source_ref in source_refs {
        hasher.update((source_ref.len() as u64).to_be_bytes());
        hasher.update(source_ref.as_bytes());
    }
    Ok(format!("fact-{}", hex::encode(hasher.finalize())))
}

fn render_value(value: &serde_json::Value) -> Result<String, String> {
    match value {
        serde_json::Value::String(value) => Ok(value.trim().to_string()),
        serde_json::Value::Null => Err("memory payload fact value cannot be null".to_string()),
        value => serde_json::to_string(value).map_err(|error| error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{MemoryPayloadContract, MEMORY_PAYLOAD_SCHEMA};

    fn contract() -> MemoryPayloadContract {
        MemoryPayloadContract {
            generation_id: "generation-1".to_string(),
            scope_kind: "thread".to_string(),
            subject_id: "agent:test".to_string(),
            project_partition_key: Some("project:alpha".to_string()),
            source_refs: vec!["record-1".to_string()],
            supersession_candidates: vec!["fact-old".to_string()],
        }
    }

    fn valid_payload() -> serde_json::Value {
        json!({
            "schema_version": MEMORY_PAYLOAD_SCHEMA,
            "scope": {
                "kind": "thread",
                "subject_id": "agent:test",
                "project_partition_key": "project:alpha"
            },
            "current_goal": null,
            "completed": [],
            "decisions": [{
                "fact_id": "model-local",
                "fact_key": "architecture.current",
                "value": "use the new architecture",
                "status": "active",
                "supersedes": ["fact-old"],
                "source_refs": ["record-1"],
                "observed_at": "2026-11-09T00:00:00Z",
                "confidence": "explicit_user"
            }],
            "open_items": [],
            "blockers": [],
            "artifacts": [],
            "constraints": [],
            "stable_preferences": [],
            "uncertainties": []
        })
    }

    #[test]
    fn payload_contract_requires_scope_appropriate_frozen_partition() {
        let mut value = contract();
        assert!(value.validate().is_ok());

        value.project_partition_key = None;
        assert!(value.validate().is_err());
        value.scope_kind = "project".to_string();
        value.project_partition_key = Some("__none__".to_string());
        assert!(value.validate().is_err());
        value.scope_kind = "agent".to_string();
        value.project_partition_key = Some("project:alpha".to_string());
        assert!(value.validate().is_err());
        value.project_partition_key = None;
        assert!(value.validate().is_ok());
    }

    #[test]
    fn finalization_replaces_model_fact_id_and_keeps_frozen_provenance() {
        let payload = valid_payload();
        let finalized = contract()
            .finalize_model_output(payload.to_string().as_str())
            .expect("valid structured payload");
        let fact = &finalized["decisions"][0];

        assert!(fact["fact_id"]
            .as_str()
            .is_some_and(|value| value.starts_with("fact-") && value != "model-local"));
        assert_eq!(fact["source_refs"], json!(["record-1"]));
        assert_eq!(fact["supersedes"], json!(["fact-old"]));
    }

    #[test]
    fn finalization_rejects_schema_scope_provenance_and_fact_field_drift() {
        let mut invalid_payloads = Vec::new();

        let mut extra = valid_payload();
        extra["unexpected"] = json!(true);
        invalid_payloads.push(extra);

        let mut wrong_scope = valid_payload();
        wrong_scope["scope"]["project_partition_key"] = json!("project:beta");
        invalid_payloads.push(wrong_scope);

        let mut wrong_ref = valid_payload();
        wrong_ref["decisions"][0]["source_refs"] = json!(["record-2"]);
        invalid_payloads.push(wrong_ref);

        let mut wrong_supersession = valid_payload();
        wrong_supersession["decisions"][0]["supersedes"] = json!(["fact-unknown"]);
        invalid_payloads.push(wrong_supersession);

        let mut wrong_status = valid_payload();
        wrong_status["decisions"][0]["status"] = json!("done");
        invalid_payloads.push(wrong_status);

        let mut wrong_confidence = valid_payload();
        wrong_confidence["decisions"][0]["confidence"] = json!("guessed");
        invalid_payloads.push(wrong_confidence);

        let mut wrong_time = valid_payload();
        wrong_time["decisions"][0]["observed_at"] = json!("yesterday");
        invalid_payloads.push(wrong_time);

        for payload in invalid_payloads {
            assert!(contract()
                .finalize_model_output(payload.to_string().as_str())
                .is_err());
        }
    }
}
