// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::ClientStorageError;
use chatos_local_agent_protocol::LOCAL_AGENT_MAX_INPUT_BYTES;

pub(super) fn append_instruction(
    input_json: &str,
    retry_instruction: Option<&str>,
) -> Result<String, ClientStorageError> {
    let mut input: serde_json::Value = serde_json::from_str(input_json)
        .map_err(|error| ClientStorageError::InvalidState(error.to_string()))?;
    if let Some(instruction) = retry_instruction {
        let object = input.as_object_mut().ok_or_else(|| {
            ClientStorageError::InvalidState("task input must be a JSON object".to_string())
        })?;
        let instructions = object
            .entry("retry_instructions")
            .or_insert_with(|| serde_json::Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| {
                ClientStorageError::InvalidState(
                    "task retry_instructions must be an array".to_string(),
                )
            })?;
        instructions.push(serde_json::Value::String(instruction.to_string()));
    }
    let encoded = serde_json::to_string(&input)
        .map_err(|error| ClientStorageError::InvalidState(error.to_string()))?;
    if encoded.len() > LOCAL_AGENT_MAX_INPUT_BYTES {
        return Err(ClientStorageError::InvalidState(format!(
            "task input exceeds the {LOCAL_AGENT_MAX_INPUT_BYTES} byte limit"
        )));
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_instructions_without_discarding_task_input() {
        let encoded = append_instruction(
            r#"{"objective":"ship","retry_instructions":["first"]}"#,
            Some("second"),
        )
        .expect("append instruction");
        let value: serde_json::Value = serde_json::from_str(&encoded).expect("decode");
        assert_eq!(value["objective"], "ship");
        assert_eq!(
            value["retry_instructions"],
            serde_json::json!(["first", "second"])
        );
    }
}
