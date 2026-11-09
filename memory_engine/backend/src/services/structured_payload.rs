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
        if self.scope_kind == "agent" && self.project_partition_key.is_some() {
            return Err(
                "agent memory payload contract cannot have a project partition".to_string(),
            );
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
        if scope.get("kind").and_then(serde_json::Value::as_str) != Some(self.scope_kind.as_str())
            || scope.get("subject_id").and_then(serde_json::Value::as_str)
                != Some(self.subject_id.as_str())
            || scope
                .get("project_partition_key")
                .and_then(serde_json::Value::as_str)
                != self.project_partition_key.as_deref()
        {
            return Err("model memory payload scope does not match frozen generation".to_string());
        }
        let allowed_refs = self.source_refs.iter().collect::<HashSet<_>>();
        let allowed_supersedes = self.supersession_candidates.iter().collect::<HashSet<_>>();
        let mut fact_count = 0usize;
        for collection in FACT_COLLECTIONS {
            let facts = payload
                .get_mut(collection)
                .and_then(serde_json::Value::as_array_mut)
                .ok_or_else(|| format!("memory payload {collection} must be an array"))?;
            for fact in facts {
                let object = fact
                    .as_object_mut()
                    .ok_or_else(|| format!("memory payload {collection} fact must be an object"))?;
                let fact_key = required_model_text(object.get("fact_key"), "fact_key")?;
                let value = object
                    .get("value")
                    .filter(|value| value.as_str().is_some_and(|value| !value.trim().is_empty()))
                    .ok_or_else(|| "model memory fact value is missing".to_string())?;
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
                let fact_id = deterministic_fact_id(
                    self.generation_id.as_str(),
                    collection,
                    fact_key,
                    value,
                    &source_refs,
                )?;
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
    match object.get("current_goal") {
        Some(value) if value.is_null() || value.is_string() => {}
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
