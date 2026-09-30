// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use crate::{LocalToolExecutor, LocalToolRegistry};
use async_trait::async_trait;
use chatos_local_agent_protocol::{
    CreateNotepadNoteCommand, GetNotepadNoteCommand, HostCommand, HostRequestEnvelope, HostResult,
    ListNotepadFoldersCommand, ListNotepadNotesCommand, LocalAgentToolInvocationRecord,
    LocalAgentToolOutcome, UpdateNotepadNoteCommand, LOCAL_AGENT_PROTOCOL_VERSION,
};
use chatos_local_agent_runtime::LocalAgentRuntime;
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use std::sync::Arc;

pub const NOTEPAD_LIST_FOLDERS_TOOL: &str = "notepad_list_folders";
pub const NOTEPAD_LIST_NOTES_TOOL: &str = "notepad_list_notes";
pub const NOTEPAD_READ_NOTE_TOOL: &str = "notepad_read_note";
pub const NOTEPAD_CREATE_NOTE_TOOL: &str = "notepad_create_note";
pub const NOTEPAD_UPDATE_NOTE_TOOL: &str = "notepad_update_note";

pub const NOTEPAD_TOOL_NAMES: [&str; 5] = [
    NOTEPAD_LIST_FOLDERS_TOOL,
    NOTEPAD_LIST_NOTES_TOOL,
    NOTEPAD_READ_NOTE_TOOL,
    NOTEPAD_CREATE_NOTE_TOOL,
    NOTEPAD_UPDATE_NOTE_TOOL,
];

pub const NOTEPAD_READ_ONLY_TOOLS: [&str; 3] = [
    NOTEPAD_LIST_FOLDERS_TOOL,
    NOTEPAD_LIST_NOTES_TOOL,
    NOTEPAD_READ_NOTE_TOOL,
];

#[derive(Clone)]
pub struct LocalNotepadToolExecutor {
    runtime: Arc<LocalAgentRuntime>,
    owner_user_id: String,
}

impl LocalNotepadToolExecutor {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
    ) -> Result<Self, String> {
        let owner_user_id = owner_user_id.into().trim().to_string();
        if owner_user_id.is_empty() || owner_user_id.len() > 256 {
            return Err("Local Notepad tool owner must be 1..=256 characters".to_string());
        }
        Ok(Self {
            runtime,
            owner_user_id,
        })
    }

    pub fn register_into(&self, registry: &mut LocalToolRegistry) -> Result<(), String> {
        let executor: Arc<dyn LocalToolExecutor> = Arc::new(self.clone());
        for name in NOTEPAD_TOOL_NAMES {
            registry.register_shared(name, Arc::clone(&executor))?;
        }
        Ok(())
    }

    async fn execute(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        let parent = self
            .runtime
            .get_run_for_host_worker(&invocation.run_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("parent Run not found: {}", invocation.run_id))?;
        if parent.owner_user_id != self.owner_user_id {
            return Err("Notepad tool Run does not belong to the active owner".to_string());
        }
        let owner = self.owner_user_id.clone();
        let command = match invocation.tool_name.as_str() {
            NOTEPAD_LIST_FOLDERS_TOOL => {
                decode::<EmptyArgs>(&invocation.arguments, invocation.tool_name.as_str())?;
                HostCommand::ListNotepadFolders(ListNotepadFoldersCommand {
                    owner_user_id: owner,
                })
            }
            NOTEPAD_LIST_NOTES_TOOL => {
                let args =
                    decode::<ListNotesArgs>(&invocation.arguments, invocation.tool_name.as_str())?;
                HostCommand::ListNotepadNotes(ListNotepadNotesCommand {
                    owner_user_id: owner,
                    query: args.query,
                    limit: args.limit.unwrap_or(200),
                })
            }
            NOTEPAD_READ_NOTE_TOOL => {
                let args =
                    decode::<NoteIdArgs>(&invocation.arguments, invocation.tool_name.as_str())?;
                HostCommand::GetNotepadNote(GetNotepadNoteCommand {
                    owner_user_id: owner,
                    note_id: args.id,
                })
            }
            NOTEPAD_CREATE_NOTE_TOOL => {
                let args =
                    decode::<CreateNoteArgs>(&invocation.arguments, invocation.tool_name.as_str())?;
                HostCommand::CreateNotepadNote(CreateNotepadNoteCommand {
                    owner_user_id: owner,
                    folder: args.folder.unwrap_or_default(),
                    title: args.title.unwrap_or_default(),
                    content: args.content.unwrap_or_default(),
                    tags: args.tags,
                })
            }
            NOTEPAD_UPDATE_NOTE_TOOL => {
                let args =
                    decode::<UpdateNoteArgs>(&invocation.arguments, invocation.tool_name.as_str())?;
                let current = self
                    .request(
                        invocation,
                        "read-before-update",
                        HostCommand::GetNotepadNote(GetNotepadNoteCommand {
                            owner_user_id: owner.clone(),
                            note_id: args.id.clone(),
                        }),
                    )
                    .await?;
                let HostResult::NotepadNote { detail } = current else {
                    return Err("Local Notepad returned an unexpected read result".to_string());
                };
                HostCommand::UpdateNotepadNote(UpdateNotepadNoteCommand {
                    owner_user_id: owner,
                    note_id: args.id,
                    expected_version: detail.note.version,
                    title: args.title,
                    content: args.content,
                    folder: args.folder,
                    tags: args.tags,
                })
            }
            name => return Err(format!("unsupported local Notepad tool: {name}")),
        };
        match self.request(invocation, "execute", command).await {
            Ok(result) => Ok(LocalAgentToolOutcome::Succeeded {
                output: output_for(result)?,
            }),
            Err(error) => Ok(LocalAgentToolOutcome::Failed {
                error,
                detail: json!({"phase": "local_notepad_tool"}),
            }),
        }
    }

    async fn request(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
        phase: &str,
        command: HostCommand,
    ) -> Result<HostResult, String> {
        self.runtime
            .try_handle(HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: format!("internal-notepad-{}-{phase}", invocation.invocation_id),
                command,
            })
            .await
            .map_err(|error| error.to_string())
    }
}

#[async_trait]
impl LocalToolExecutor for LocalNotepadToolExecutor {
    async fn execute_tool(
        &self,
        invocation: &LocalAgentToolInvocationRecord,
    ) -> Result<LocalAgentToolOutcome, String> {
        Ok(match self.execute(invocation).await {
            Ok(outcome) => outcome,
            Err(error) => LocalAgentToolOutcome::Failed {
                error,
                detail: json!({"phase": "local_notepad_tool_validation"}),
            },
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListNotesArgs {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteIdArgs {
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateNoteArgs {
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateNoteArgs {
    id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

fn decode<T: DeserializeOwned>(arguments: &Value, tool_name: &str) -> Result<T, String> {
    serde_json::from_value(arguments.clone())
        .map_err(|error| format!("invalid {tool_name} input: {error}"))
}

fn output_for(result: HostResult) -> Result<Value, String> {
    match result {
        HostResult::NotepadFolders { folders } => Ok(json!({"folders": folders})),
        HostResult::NotepadNotes { notes } => Ok(json!({"notes": notes})),
        HostResult::NotepadNote { detail } => serde_json::to_value(detail)
            .map_err(|error| format!("failed to encode Notepad result: {error}")),
        _ => Err("Local Notepad returned an unexpected tool result".to_string()),
    }
}

pub fn notepad_model_tools() -> Vec<Value> {
    vec![
        model_tool(
            NOTEPAD_LIST_FOLDERS_TOOL,
            "List all folders in the current user's local notepad.",
            json!({"type": "object", "properties": {}, "additionalProperties": false}),
        ),
        model_tool(
            NOTEPAD_LIST_NOTES_TOOL,
            "List or search the current user's local notes.",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 500}
                },
                "additionalProperties": false
            }),
        ),
        model_tool(
            NOTEPAD_READ_NOTE_TOOL,
            "Read one local note by id.",
            json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"],
                "additionalProperties": false
            }),
        ),
        model_tool(
            NOTEPAD_CREATE_NOTE_TOOL,
            "Create a markdown note in the current user's local notepad.",
            note_write_schema(false),
        ),
        model_tool(
            NOTEPAD_UPDATE_NOTE_TOOL,
            "Update a local note by id. Supply at least one field to change.",
            note_write_schema(true),
        ),
    ]
}

fn note_write_schema(update: bool) -> Value {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "folder": {"type": "string"},
            "title": {"type": "string"},
            "content": {"type": "string"},
            "tags": {"type": "array", "items": {"type": "string"}, "maxItems": 64}
        },
        "additionalProperties": false
    });
    if update {
        schema["properties"]["id"] = json!({"type": "string"});
        schema["required"] = json!(["id"]);
    }
    schema
}

fn model_tool(name: &str, description: &str, parameters: Value) -> Value {
    json!({
        "type": "function",
        "name": name,
        "description": description,
        "parameters": parameters
    })
}
