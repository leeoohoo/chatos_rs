// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use chatos_local_agent_protocol::{
    canonical_json_digest, LocalAgentRun, ModelStepResult, ToolEffect,
};
use chatos_local_agent_runtime::{LocalAgentProfile, LocalAgentProfileStep, ModelGatewayOutput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::shared::{parse_tool_arguments, validate_context_strategy};

pub const STORY_DESIGN_PROFILE_KEY: &str = "story_design";
pub const STORY_DESIGN_PROMPT_REVISION: &str = "story-design-v1";
pub const STORY_DESIGN_CAPABILITY_SNAPSHOT_REF: &str = "story-design-tools-v1";
pub const STORY_FINISH_TOOL: &str = "story_finish";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoryDesignStage {
    Outline,
    Refine,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoryDesignState {
    pub schema_version: u32,
    pub story_record_id: String,
    pub project_id: String,
    pub base_project_revision: u64,
    pub base_project_digest: String,
    pub stage: StoryDesignStage,
    pub target_ids: Vec<String>,
    pub draft: Value,
    pub read_through: usize,
    pub read_segment_ids: Vec<String>,
}

impl StoryDesignState {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("story design state schema is unsupported".to_string());
        }
        for (field, value) in [
            ("story record ID", self.story_record_id.as_str()),
            ("project ID", self.project_id.as_str()),
            ("base project digest", self.base_project_digest.as_str()),
        ] {
            if value.trim().is_empty() || value.trim() != value {
                return Err(format!("{field} is invalid"));
            }
        }
        let project = project_object(&self.draft)?;
        if string_field(project, "id")? != self.project_id {
            return Err("story draft does not match the frozen project ID".to_string());
        }
        if integer_field(project, "version")? != 2 {
            return Err("story project schema is unsupported".to_string());
        }
        let source_length = character_count(string_field(project, "source")?);
        if self.read_through > source_length {
            return Err("story read cursor exceeds the frozen source".to_string());
        }
        if self.target_ids.len() > 200
            || self.target_ids.iter().any(|value| value.trim().is_empty())
            || self.target_ids.iter().collect::<HashSet<_>>().len() != self.target_ids.len()
        {
            return Err("story target IDs are invalid".to_string());
        }
        if self.read_segment_ids.len() > 200
            || self
                .read_segment_ids
                .iter()
                .any(|value| value.trim().is_empty())
            || self.read_segment_ids.iter().collect::<HashSet<_>>().len()
                != self.read_segment_ids.len()
        {
            return Err("story read segment IDs are invalid".to_string());
        }
        let segments = array_field(project, "segments")?;
        match self.stage {
            StoryDesignStage::Outline if !self.target_ids.is_empty() => {
                return Err("outline planning cannot carry target IDs".to_string());
            }
            StoryDesignStage::Refine if self.target_ids.is_empty() => {
                return Err("refine planning requires target IDs".to_string());
            }
            StoryDesignStage::Refine
                if self.target_ids.iter().any(|target| {
                    !segments
                        .iter()
                        .any(|segment| object_string(segment, "id") == Some(target.as_str()))
                }) =>
            {
                return Err("refine target is absent from the frozen draft".to_string());
            }
            _ => {}
        }
        validate_project_shape(project)
    }

    pub fn user_prompt(&self) -> Result<String, String> {
        let source_length = character_count(string_field(project_object(&self.draft)?, "source")?);
        Ok(match self.stage {
            StoryDesignStage::Outline => format!(
                "Plan the story outline incrementally. Read the complete source in bounded pages, build reusable character and scene profiles, append contiguous story segments, save relations for every segment, then finish only after validation. source_length={source_length}."
            ),
            StoryDesignStage::Refine => format!(
                "Refine exactly the authorized story segments incrementally. Read every target segment before writing its complete shot plan, preserve its duration and references, then finish only after validation. target_count={}. source_length={source_length}.",
                self.target_ids.len()
            ),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoryDesignStepContext {
    pub state: StoryDesignState,
    pub model_input_items: Vec<Value>,
    pub maximum_output_tokens: u32,
    pub native_compaction_threshold: Option<u64>,
    pub memory_engine_active_threshold: Option<u64>,
    pub maximum_summary_attempts: u8,
}

#[async_trait]
pub trait StoryDesignContextProvider: Send + Sync {
    async fn load_step_context(
        &self,
        run: &LocalAgentRun,
    ) -> Result<StoryDesignStepContext, String>;
}

pub struct StoryDesignAgentProfile {
    context_provider: Arc<dyn StoryDesignContextProvider>,
}

impl StoryDesignAgentProfile {
    pub fn new(context_provider: Arc<dyn StoryDesignContextProvider>) -> Self {
        Self { context_provider }
    }
}

#[async_trait]
impl LocalAgentProfile for StoryDesignAgentProfile {
    fn profile_key(&self) -> &'static str {
        STORY_DESIGN_PROFILE_KEY
    }

    async fn prepare_model_step(
        &self,
        run: &LocalAgentRun,
    ) -> Result<LocalAgentProfileStep, String> {
        let context = self.context_provider.load_step_context(run).await?;
        validate_story_context(run, &context)?;
        Ok(LocalAgentProfileStep {
            model_input_items: context.model_input_items,
            tools: story_tool_definitions(context.state.stage)
                .into_iter()
                .map(|definition| definition.schema)
                .collect(),
            instructions: Some(story_system_prompt().to_string()),
            maximum_output_tokens: context.maximum_output_tokens,
            reasoning_effort: None,
            temperature: None,
            native_compaction_threshold: context.native_compaction_threshold,
            memory_engine_active_threshold: context.memory_engine_active_threshold,
            maximum_summary_attempts: context.maximum_summary_attempts,
        })
    }

    async fn interpret_completed_output(
        &self,
        run: &LocalAgentRun,
        output: &ModelGatewayOutput,
    ) -> Result<ModelStepResult, String> {
        let context = self.context_provider.load_step_context(run).await?;
        validate_story_context(run, &context)?;
        interpret_story_output(output, &context.state)
    }
}

fn validate_story_context(
    run: &LocalAgentRun,
    context: &StoryDesignStepContext,
) -> Result<(), String> {
    if run.profile_key != STORY_DESIGN_PROFILE_KEY
        || run.owner_entity_type != "story_design"
        || run.owner_entity_id != context.state.story_record_id
        || run.project_id.as_deref() != Some(context.state.project_id.as_str())
        || run.prompt_revision != STORY_DESIGN_PROMPT_REVISION
        || run.capability_snapshot_ref != STORY_DESIGN_CAPABILITY_SNAPSHOT_REF
    {
        return Err("story design context does not match the durable Run".to_string());
    }
    context.state.validate()?;
    validate_context_strategy(
        run,
        context.native_compaction_threshold,
        context.memory_engine_active_threshold,
        context.maximum_summary_attempts,
    )
}

fn story_system_prompt() -> &'static str {
    "You are ChatOS's story visual-design planner. Work only on story structure, visual continuity, reusable character and scene appearance, shot composition, camera language, lighting, palette, audio notes, and generation constraints. Do not generate media, execute external actions, or redesign product interaction. Work incrementally with the provided tools. Read only the bounded information you need, persist each useful planning step, never invent IDs outside the frozen project, and call story_finish alone only when story_read_state reports readyToFinish=true."
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoryToolDefinition {
    pub name: &'static str,
    pub effect: ToolEffect,
    pub schema: Value,
}

pub fn story_tool_definitions(stage: StoryDesignStage) -> Vec<StoryToolDefinition> {
    let mut tools = vec![
        tool("story_read_state", ToolEffect::Read, "Read bounded story planning progress and the next required action.", json!({"offset": integer_schema(0, 200)})),
        tool("story_read_graph", ToolEffect::Read, "Read a bounded page of story characters, scenes, props, segments, and relations.", json!({"nodeOffset": integer_schema(0, 400), "edgeOffset": integer_schema(0, 4000), "limit": integer_schema(1, 50)})),
        tool("story_read_source", ToolEffect::Read, "Read the frozen source in bounded pages.", json!({"offset": integer_schema(0, 80000), "limit": integer_schema(1, 1200)})),
        tool("story_read_text", ToolEffect::Read, "Read the complete description, style, or summary in bounded pages.", json!({"field": {"type":"string","enum":["description","style","summary"]}, "offset": integer_schema(0, 16000), "limit": integer_schema(1, 1200)})),
        tool("story_read_segment", ToolEffect::Read, "Read one segment, its source excerpt, references, relations, and adjacent continuity.", json!({"segmentID": text_schema(1, 128)})),
        tool("story_read_asset", ToolEffect::Read, "Read one character, scene, or prop definition.", json!({"assetID": text_schema(1, 128)})),
        tool("story_read_asset_prompt", ToolEffect::Read, "Read an asset prompt in bounded pages.", json!({"assetID": text_schema(1, 128), "offset": integer_schema(0, 4000), "limit": integer_schema(1, 1200)})),
        tool("story_save_segment_relations", ToolEffect::DraftWrite, "Save a segment's character, scene, prop, and character-scene relations into the reversible draft.", json!({
            "segmentID": text_schema(1,128),
            "characterIDs":{"type":"array","items":text_schema(1,128),"maxItems":8},
            "sceneIDs":{"type":"array","items":text_schema(1,128),"minItems":1,"maxItems":8},
            "propIDs":{"type":"array","items":text_schema(1,128),"maxItems":8},
            "relations":{"type":"array","maxItems":8,"items":object_schema(json!({
                "relationID":text_schema(1,128),"characterID":text_schema(1,128),"sceneID":text_schema(1,128),
                "action":text_schema(1,200),"position":text_schema(1,200),
                "startSecond":integer_schema(0,14),"endSecond":integer_schema(1,15)
            }))}
        })),
    ];
    match stage {
        StoryDesignStage::Outline => tools.extend([
            tool("story_save_summary", ToolEffect::DraftWrite, "Save the story summary into the reversible draft.", json!({"summary":text_schema(1,2000)})),
            tool("story_upsert_asset", ToolEffect::DraftWrite, "Save one prop definition into the reversible draft.", json!({"id":text_schema(1,128),"kind":{"type":"string","enum":["prop"]},"name":text_schema(1,120),"prompt":text_schema(1,1500)})),
            tool("story_save_scene_profile", ToolEffect::DraftWrite, "Save one reusable scene visual profile into the reversible draft.", json!({"id":text_schema(1,128),"name":text_schema(1,120),"profile":object_schema(json!({
                "roleInStory":text_schema(1,400),"setting":text_schema(1,400),"spatialLayout":text_schema(1,400),
                "lightingAndPalette":text_schema(1,400),"keyElements":text_schema(1,400),"atmosphere":text_schema(1,400),"consistencyNotes":text_schema(1,400)
            }))})),
            tool("story_save_character_profile", ToolEffect::DraftWrite, "Save one reusable character visual profile into the reversible draft.", json!({"id":text_schema(1,128),"name":text_schema(1,120),"profile":object_schema(json!({
                "isProtagonist":{"type":"boolean"},"roleInStory":text_schema(1,400),"appearance":text_schema(1,400),
                "personality":text_schema(1,400),"motivation":text_schema(1,400),"relationships":text_schema(1,400),
                "costume":text_schema(1,400),"consistencyNotes":text_schema(1,400)
            }))})),
            tool("story_append_segments", ToolEffect::DraftWrite, "Append up to five contiguous story or transition segments to the reversible draft.", json!({"segments":{"type":"array","minItems":1,"maxItems":5,"items":object_schema(json!({
                "id":text_schema(1,128),"title":text_schema(1,120),"synopsis":text_schema(1,250),
                "kind":{"type":"string","enum":["story","transition"]},"seconds":integer_schema(2,15),
                "sourceStart":integer_schema(0,80000),"sourceEnd":integer_schema(0,80000)
            }))}})),
        ]),
        StoryDesignStage::Refine => tools.push(tool("story_update_segment", ToolEffect::DraftWrite, "Save the complete visual shot language for one authorized segment.", json!({
            "segmentID":text_schema(1,128),"detail":object_schema(json!({
                "firstFramePrompt":text_schema(1,500),"lastFramePrompt":text_schema(1,500),
                "shots":{"type":"array","minItems":1,"maxItems":8,"items":object_schema(json!({"start":integer_schema(0,14),"end":integer_schema(1,15),"prompt":text_schema(1,200)}))},
                "continuityIn":text_schema(0,200),"continuityOut":text_schema(0,200),"audio":text_schema(0,200),"constraints":text_schema(0,300)
            }))
        }))),
    }
    tools.push(tool(
        STORY_FINISH_TOOL,
        ToolEffect::Terminal,
        "Validate and finish this planning stage. Call alone.",
        json!({}),
    ));
    tools
}

fn tool(
    name: &'static str,
    effect: ToolEffect,
    description: &'static str,
    fields: Value,
) -> StoryToolDefinition {
    StoryToolDefinition {
        name,
        effect,
        schema: json!({"type":"function","name":name,"description":description,"parameters":object_schema(fields)}),
    }
}

fn object_schema(fields: Value) -> Value {
    let required = fields
        .as_object()
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    json!({"type":"object","properties":fields,"required":required,"additionalProperties":false})
}

fn text_schema(minimum: usize, maximum: usize) -> Value {
    json!({"type":"string","minLength":minimum,"maxLength":maximum})
}

fn integer_schema(minimum: i64, maximum: i64) -> Value {
    json!({"type":"integer","minimum":minimum,"maximum":maximum})
}

fn interpret_story_output(
    output: &ModelGatewayOutput,
    state: &StoryDesignState,
) -> Result<ModelStepResult, String> {
    let calls = output
        .terminal
        .output_items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("function_call"))
        .collect::<Vec<_>>();
    if calls.is_empty() {
        return Ok(ModelStepResult::Continue(json!({
            "reason":"story_design_requires_a_tool_call",
            "progress_text":output.content
        })));
    }
    if calls.len() == 1 && calls[0].get("name").and_then(Value::as_str) == Some(STORY_FINISH_TOOL) {
        let arguments = parse_tool_arguments(calls[0])?;
        if arguments.as_object().is_none_or(|value| !value.is_empty()) {
            return Err("story_finish arguments must be an empty object".to_string());
        }
        validate_story_completion(state)?;
        return Ok(ModelStepResult::Final(json!({
            "kind":"story_design",
            "story_record_id":state.story_record_id,
            "project_id":state.project_id,
            "base_project_revision":state.base_project_revision,
            "base_project_digest":state.base_project_digest,
            "stage":state.stage,
            "draft_digest":canonical_json_digest(&state.draft),
        })));
    }
    let definitions = story_tool_definitions(state.stage)
        .into_iter()
        .map(|definition| (definition.name, definition.effect))
        .collect::<HashMap<_, _>>();
    let mut commands = Vec::with_capacity(calls.len());
    let mut call_ids = HashSet::new();
    for call in calls {
        let name = call
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "story tool call has no name".to_string())?;
        if name == STORY_FINISH_TOOL {
            return Err("story_finish must be the only tool call in its response".to_string());
        }
        let effect = definitions
            .get(name)
            .ok_or_else(|| format!("story design emitted unauthorized tool {name}"))?;
        let call_id = call
            .get("call_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| format!("story tool {name} has no call_id"))?;
        if !call_ids.insert(call_id.to_string()) {
            return Err(format!("story tool call_id {call_id} is duplicated"));
        }
        commands.push(json!({
            "call_id":call_id,
            "name":name,
            "effect":effect,
            "arguments":parse_tool_arguments(call)?
        }));
    }
    Ok(ModelStepResult::ToolCommand(json!({
        "project_id":state.project_id,
        "capability_snapshot_ref":STORY_DESIGN_CAPABILITY_SNAPSHOT_REF,
        "calls":commands,
    })))
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoryToolResult {
    pub bounded_result: Value,
    pub changed: bool,
}

pub fn execute_story_tool(
    state: &mut StoryDesignState,
    name: &str,
    arguments: &Value,
) -> Result<StoryToolResult, String> {
    state.validate()?;
    if name == STORY_FINISH_TOOL {
        return Err("story_finish is interpreted by the profile and is never executed".to_string());
    }
    let mut next = state.clone();
    let result = match name {
        "story_read_state" => read_state(&next, arguments)?,
        "story_read_graph" => read_graph(&next, arguments)?,
        "story_read_source" => read_source(&mut next, arguments)?,
        "story_read_text" => read_text(&next, arguments)?,
        "story_read_segment" => read_segment(&mut next, arguments)?,
        "story_read_asset" => read_asset(&next, arguments)?,
        "story_read_asset_prompt" => read_asset_prompt(&next, arguments)?,
        "story_save_summary" => save_summary(&mut next, arguments)?,
        "story_upsert_asset" => upsert_prop(&mut next, arguments)?,
        "story_save_character_profile" => save_profile(&mut next, arguments, "characters")?,
        "story_save_scene_profile" => save_profile(&mut next, arguments, "scenes")?,
        "story_append_segments" => append_segments(&mut next, arguments)?,
        "story_save_segment_relations" => save_relations(&mut next, arguments)?,
        "story_update_segment" => update_segment(&mut next, arguments)?,
        _ => {
            return Err(format!(
                "story tool {name} is not registered for this stage"
            ))
        }
    };
    next.validate()?;
    *state = next;
    Ok(result)
}

fn read_state(state: &StoryDesignState, arguments: &Value) -> Result<StoryToolResult, String> {
    let offset = usize_argument(arguments, "offset", 0, 200)?;
    let project = project_object(&state.draft)?;
    let segments = array_field(project, "segments")?;
    let characters = array_field(project, "characters")?;
    let scenes = array_field(project, "scenes")?;
    let props = array_field(project, "props")?;
    let remaining = state
        .target_ids
        .iter()
        .filter(|target| {
            segments
                .iter()
                .find(|segment| object_string(segment, "id") == Some(target.as_str()))
                .and_then(|segment| segment.get("detail"))
                .is_none_or(Value::is_null)
        })
        .cloned()
        .collect::<Vec<_>>();
    let ready = validate_story_completion(state).is_ok();
    let resources = characters
        .iter()
        .map(|value| resource_index(value, "character"))
        .chain(scenes.iter().map(|value| resource_index(value, "scene")))
        .chain(props.iter().map(|value| resource_index(value, "prop")))
        .skip(offset)
        .take(5)
        .collect::<Vec<_>>();
    let segment_page = segments
        .iter()
        .skip(offset)
        .take(5)
        .map(|segment| {
            json!({
                "id":segment.get("id"),
                "title":segment.get("title"),
                "kind":segment.get("kind"),
                "seconds":segment.get("seconds"),
                "detailSaved":segment.get("detail").is_some_and(|value| !value.is_null())
            })
        })
        .collect::<Vec<_>>();
    Ok(read_result(json!({
        "stage":state.stage,
        "title":project.get("title"),
        "description":preview(project.get("description"),500),
        "style":preview(project.get("style"),500),
        "ratio":project.get("ratio"),
        "summary":preview(project.get("summary"),500),
        "sourceLength":character_count(string_field(project,"source")?),
        "readThrough":state.read_through,
        "coveredThrough":segments.last().and_then(|segment| segment.pointer("/sourceRange/end")).and_then(Value::as_u64).unwrap_or(0),
        "characterCount":characters.len(),
        "sceneCount":scenes.len(),
        "propCount":props.len(),
        "segmentCount":segments.len(),
        "targetCount":state.target_ids.len(),
        "targetIDs":state.target_ids.iter().skip(offset).take(5).collect::<Vec<_>>(),
        "readyToFinish":ready,
        "remainingTargetCount":remaining.len(),
        "remainingTargetIDs":remaining.into_iter().take(20).collect::<Vec<_>>(),
        "nextAction":if ready {"Call story_finish alone."} else {"Complete only the remaining stage requirements; do not restart."},
        "resources":resources,
        "segments":segment_page,
        "nextOffset":offset.saturating_add(5)
    })))
}

fn read_graph(state: &StoryDesignState, arguments: &Value) -> Result<StoryToolResult, String> {
    let node_offset = usize_argument(arguments, "nodeOffset", 0, 400)?;
    let edge_offset = usize_argument(arguments, "edgeOffset", 0, 4_000)?;
    let limit = usize_argument(arguments, "limit", 1, 50)?;
    let project = project_object(&state.draft)?;
    let nodes = ["characters", "scenes", "props", "segments"]
        .into_iter()
        .flat_map(|key| array_field(project, key).cloned().unwrap_or_default())
        .skip(node_offset)
        .take(limit)
        .collect::<Vec<_>>();
    let edges = array_field(project, "relations")?
        .iter()
        .skip(edge_offset)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    Ok(read_result(json!({
        "nodes":nodes,
        "edges":edges,
        "nextNodeOffset":node_offset+limit,
        "nextEdgeOffset":edge_offset+limit
    })))
}

fn read_source(state: &mut StoryDesignState, arguments: &Value) -> Result<StoryToolResult, String> {
    let offset = usize_argument(arguments, "offset", 0, 80_000)?;
    let limit = usize_argument(arguments, "limit", 1, 1_200)?;
    let source = string_field(project_object(&state.draft)?, "source")?;
    let length = character_count(source);
    if offset >= length || (state.stage == StoryDesignStage::Outline && offset > state.read_through)
    {
        return Err("story source page is outside the authorized read cursor".to_string());
    }
    let text = character_slice(source, offset, limit);
    let next = offset + character_count(&text);
    let changed = next > state.read_through;
    if offset <= state.read_through {
        state.read_through = state.read_through.max(next);
    }
    Ok(StoryToolResult {
        bounded_result: json!({"offset":offset,"nextOffset":next,"sourceLength":length,"text":text}),
        changed,
    })
}

fn read_text(state: &StoryDesignState, arguments: &Value) -> Result<StoryToolResult, String> {
    let field = string_argument(arguments, "field")?;
    if !matches!(field, "description" | "style" | "summary") {
        return Err("story text field is not readable".to_string());
    }
    let offset = usize_argument(arguments, "offset", 0, 16_000)?;
    let limit = usize_argument(arguments, "limit", 1, 1_200)?;
    let value = string_field(project_object(&state.draft)?, field)?;
    let length = character_count(value);
    if offset > length {
        return Err("story text offset exceeds the field".to_string());
    }
    let text = character_slice(value, offset, limit);
    Ok(read_result(json!({
        "field":field,
        "offset":offset,
        "nextOffset":offset+character_count(&text),
        "length":length,
        "text":text
    })))
}

fn read_segment(
    state: &mut StoryDesignState,
    arguments: &Value,
) -> Result<StoryToolResult, String> {
    let id = string_argument(arguments, "segmentID")?;
    let project = project_object(&state.draft)?;
    let segments = array_field(project, "segments")?;
    let index = segments
        .iter()
        .position(|segment| object_string(segment, "id") == Some(id))
        .ok_or_else(|| "story segment is outside the frozen draft".to_string())?;
    let segment = &segments[index];
    let start = segment
        .pointer("/sourceRange/start")
        .and_then(Value::as_u64)
        .ok_or_else(|| "story segment source range is invalid".to_string())?
        as usize;
    let end = segment
        .pointer("/sourceRange/end")
        .and_then(Value::as_u64)
        .ok_or_else(|| "story segment source range is invalid".to_string())? as usize;
    let source = string_field(project, "source")?;
    let excerpt = character_slice(source, start, end.saturating_sub(start));
    let relations = array_field(project, "relations")?
        .iter()
        .filter(|relation| object_string(relation, "segmentID") == Some(id))
        .cloned()
        .collect::<Vec<_>>();
    let result = json!({
        "segment":segment,
        "sourceExcerpt":excerpt,
        "relations":relations,
        "previous":index.checked_sub(1).and_then(|value|segments.get(value)),
        "next":segments.get(index+1)
    });
    let changed = !state.read_segment_ids.iter().any(|value| value == id);
    if changed {
        state.read_segment_ids.push(id.to_string());
    }
    Ok(StoryToolResult {
        bounded_result: result,
        changed,
    })
}

fn read_asset(state: &StoryDesignState, arguments: &Value) -> Result<StoryToolResult, String> {
    let id = string_argument(arguments, "assetID")?;
    let (kind, asset) = find_asset(project_object(&state.draft)?, id)
        .ok_or_else(|| "story asset is outside the frozen draft".to_string())?;
    Ok(read_result(json!({"kind":kind,"asset":asset})))
}

fn read_asset_prompt(
    state: &StoryDesignState,
    arguments: &Value,
) -> Result<StoryToolResult, String> {
    let id = string_argument(arguments, "assetID")?;
    let offset = usize_argument(arguments, "offset", 0, 4_000)?;
    let limit = usize_argument(arguments, "limit", 1, 1_200)?;
    let (_, asset) = find_asset(project_object(&state.draft)?, id)
        .ok_or_else(|| "story asset is outside the frozen draft".to_string())?;
    let prompt = asset
        .get("imagePrompt")
        .and_then(Value::as_str)
        .ok_or_else(|| "story asset prompt is invalid".to_string())?;
    let length = character_count(prompt);
    if offset > length {
        return Err("story asset prompt offset exceeds the field".to_string());
    }
    let text = character_slice(prompt, offset, limit);
    Ok(read_result(json!({
        "assetID":id,
        "offset":offset,
        "nextOffset":offset+character_count(&text),
        "length":length,
        "text":text
    })))
}

fn save_summary(
    state: &mut StoryDesignState,
    arguments: &Value,
) -> Result<StoryToolResult, String> {
    require_stage(state, StoryDesignStage::Outline)?;
    let summary = string_argument(arguments, "summary")?;
    if summary.trim().is_empty() || character_count(summary) > 4_000 {
        return Err("story summary is invalid".to_string());
    }
    let project = project_object_mut(&mut state.draft)?;
    let changed = project.get("summary").and_then(Value::as_str) != Some(summary);
    project.insert("summary".to_string(), Value::String(summary.to_string()));
    Ok(write_result("Story summary saved.", changed))
}

fn upsert_prop(state: &mut StoryDesignState, arguments: &Value) -> Result<StoryToolResult, String> {
    require_stage(state, StoryDesignStage::Outline)?;
    if string_argument(arguments, "kind")? != "prop" {
        return Err("story asset kind is not authorized".to_string());
    }
    let id = string_argument(arguments, "id")?;
    let name = string_argument(arguments, "name")?;
    let prompt = string_argument(arguments, "prompt")?;
    reject_cross_kind_collision(project_object(&state.draft)?, id, "props")?;
    let value = json!({
        "id":id,
        "name":name,
        "description":prompt,
        "imagePrompt":prompt,
        "media":empty_media()
    });
    let array = array_field_mut(project_object_mut(&mut state.draft)?, "props")?;
    upsert_value(array, id, value, "Prop saved.")
}

fn save_profile(
    state: &mut StoryDesignState,
    arguments: &Value,
    collection: &str,
) -> Result<StoryToolResult, String> {
    require_stage(state, StoryDesignStage::Outline)?;
    if state.read_through == 0 {
        return Err("story source must be read before saving visual profiles".to_string());
    }
    let id = string_argument(arguments, "id")?;
    let name = string_argument(arguments, "name")?;
    let profile = arguments
        .get("profile")
        .cloned()
        .filter(Value::is_object)
        .ok_or_else(|| "story profile is invalid".to_string())?;
    reject_cross_kind_collision(project_object(&state.draft)?, id, collection)?;
    let image_prompt = if collection == "characters" {
        join_profile(&profile, &["appearance", "costume", "consistencyNotes"])?
    } else {
        join_profile(
            &profile,
            &[
                "setting",
                "spatialLayout",
                "lightingAndPalette",
                "keyElements",
                "atmosphere",
                "consistencyNotes",
            ],
        )?
    };
    let value = json!({
        "id":id,
        "name":name,
        "profile":profile,
        "imagePrompt":image_prompt,
        "media":empty_media()
    });
    let array = array_field_mut(project_object_mut(&mut state.draft)?, collection)?;
    upsert_value(array, id, value, "Visual profile saved.")
}

fn append_segments(
    state: &mut StoryDesignState,
    arguments: &Value,
) -> Result<StoryToolResult, String> {
    require_stage(state, StoryDesignStage::Outline)?;
    let values = arguments
        .get("segments")
        .and_then(Value::as_array)
        .ok_or_else(|| "story segments are invalid".to_string())?;
    if values.is_empty() || values.len() > 5 {
        return Err("story segment batch must contain one to five items".to_string());
    }
    let existing_segments = array_field(project_object(&state.draft)?, "segments")?;
    let mut cursor = existing_segments
        .last()
        .and_then(|segment| segment.pointer("/sourceRange/end"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let existing = existing_segments
        .iter()
        .filter_map(|segment| object_string(segment, "id"))
        .map(str::to_string)
        .collect::<HashSet<_>>();
    let mut added = Vec::new();
    let mut previous_kind = existing_segments
        .last()
        .and_then(|segment| object_string(segment, "kind"))
        .map(str::to_string);
    for value in values {
        let object = value
            .as_object()
            .ok_or_else(|| "story segment is invalid".to_string())?;
        let id = string_field(object, "id")?;
        let kind = string_field(object, "kind")?;
        let seconds = integer_field(object, "seconds")?;
        let start = integer_field(object, "sourceStart")?;
        let end = integer_field(object, "sourceEnd")?;
        if existing.contains(id)
            || added
                .iter()
                .any(|item: &Value| object_string(item, "id") == Some(id))
            || !(2..=15).contains(&seconds)
        {
            return Err("story segment identity or duration is invalid".to_string());
        }
        let valid = if kind == "story" {
            start == cursor as i64 && end > start && end as usize <= state.read_through
        } else if kind == "transition" {
            start == cursor as i64
                && end == start
                && seconds <= 3
                && previous_kind
                    .as_deref()
                    .is_some_and(|value| value != "transition")
        } else {
            false
        };
        if !valid {
            return Err("story segment does not continue the authorized source range".to_string());
        }
        added.push(json!({
            "id":id,
            "title":string_field(object,"title")?,
            "synopsis":string_field(object,"synopsis")?,
            "kind":kind,
            "seconds":seconds,
            "sourceRange":{"start":start,"end":end},
            "characterIDs":[],
            "sceneIDs":[],
            "propIDs":[],
            "detail":null,
            "firstFrames":empty_media(),
            "inheritedFirstFrameSourceSegmentID":null,
            "lastFrames":empty_media(),
            "useLastFrameForVideo":true,
            "attempt":null,
            "previousAttempts":[],
            "video":null,
            "error":null
        }));
        if kind == "story" {
            cursor = end as usize;
        }
        previous_kind = Some(kind.to_string());
    }
    array_field_mut(project_object_mut(&mut state.draft)?, "segments")?.extend(added);
    Ok(write_result(
        format!("Segments saved; source coverage now ends at {cursor}."),
        true,
    ))
}

fn save_relations(
    state: &mut StoryDesignState,
    arguments: &Value,
) -> Result<StoryToolResult, String> {
    let segment_id = string_argument(arguments, "segmentID")?;
    if state.stage == StoryDesignStage::Refine
        && !state.target_ids.iter().any(|id| id == segment_id)
    {
        return Err("story segment is outside the authorized targets".to_string());
    }
    let character_ids = string_array_argument(arguments, "characterIDs", 8)?;
    let scene_ids = string_array_argument(arguments, "sceneIDs", 8)?;
    let prop_ids = string_array_argument(arguments, "propIDs", 8)?;
    if scene_ids.is_empty() {
        return Err("every story segment requires at least one scene".to_string());
    }
    let relation_inputs = arguments
        .get("relations")
        .and_then(Value::as_array)
        .ok_or_else(|| "story relations are invalid".to_string())?;
    let mut relations = Vec::new();
    for input in relation_inputs {
        let object = input
            .as_object()
            .ok_or_else(|| "story relation is invalid".to_string())?;
        relations.push(json!({
            "id":string_field(object,"relationID")?,
            "segmentID":segment_id,
            "characterID":string_field(object,"characterID")?,
            "sceneID":string_field(object,"sceneID")?,
            "action":string_field(object,"action")?,
            "position":string_field(object,"position")?,
            "startSecond":integer_field(object,"startSecond")?,
            "endSecond":integer_field(object,"endSecond")?
        }));
    }
    let project = project_object_mut(&mut state.draft)?;
    let segments = array_field_mut(project, "segments")?;
    let segment = segments
        .iter_mut()
        .find(|value| object_string(value, "id") == Some(segment_id))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "story segment is absent".to_string())?;
    if segment.get("detail").is_some_and(|value| !value.is_null()) {
        return Err("story relations cannot change after shot detail is saved".to_string());
    }
    segment.insert("characterIDs".to_string(), json!(character_ids));
    segment.insert("sceneIDs".to_string(), json!(scene_ids));
    segment.insert("propIDs".to_string(), json!(prop_ids));
    let existing = array_field_mut(project, "relations")?;
    existing.retain(|value| object_string(value, "segmentID") != Some(segment_id));
    existing.extend(relations);
    validate_segment_relations(project, segment_id)?;
    Ok(write_result("Segment relations saved.", true))
}

fn update_segment(
    state: &mut StoryDesignState,
    arguments: &Value,
) -> Result<StoryToolResult, String> {
    require_stage(state, StoryDesignStage::Refine)?;
    let id = string_argument(arguments, "segmentID")?;
    if !state.target_ids.iter().any(|target| target == id) {
        return Err("story segment is outside the authorized targets".to_string());
    }
    if !state.read_segment_ids.iter().any(|value| value == id) {
        return Err("story segment must be read before saving its visual plan".to_string());
    }
    let detail = arguments
        .get("detail")
        .cloned()
        .filter(Value::is_object)
        .ok_or_else(|| "story segment detail is invalid".to_string())?;
    let project = project_object_mut(&mut state.draft)?;
    validate_segment_relations(project, id)?;
    let segment = array_field_mut(project, "segments")?
        .iter_mut()
        .find(|value| object_string(value, "id") == Some(id))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "story segment is absent".to_string())?;
    let duration = integer_field(segment, "seconds")?;
    validate_detail(&detail, duration)?;
    if let Some(existing) = segment.get("detail").filter(|value| !value.is_null()) {
        if existing == &detail {
            return Ok(write_result("Segment detail already saved.", false));
        }
        return Err("saved story segment detail is immutable within this Run".to_string());
    }
    segment.insert("detail".to_string(), detail);
    if let Some(media) = segment
        .get_mut("firstFrames")
        .and_then(Value::as_object_mut)
    {
        media.insert("confirmedImageID".to_string(), Value::Null);
    }
    if let Some(media) = segment.get_mut("lastFrames").and_then(Value::as_object_mut) {
        media.insert("confirmedImageID".to_string(), Value::Null);
    }
    segment.insert("useLastFrameForVideo".to_string(), Value::Bool(false));
    Ok(write_result("Segment visual plan saved.", true))
}

pub fn validate_story_completion(state: &StoryDesignState) -> Result<(), String> {
    state.validate()?;
    let project = project_object(&state.draft)?;
    let segments = array_field(project, "segments")?;
    match state.stage {
        StoryDesignStage::Outline => {
            let source_length = character_count(string_field(project, "source")?);
            if state.read_through != source_length
                || string_field(project, "summary")?.trim().is_empty()
                || segments.is_empty()
            {
                return Err("story outline is incomplete".to_string());
            }
            let mut cursor = 0usize;
            for (index, segment) in segments.iter().enumerate() {
                let id = object_string(segment, "id")
                    .ok_or_else(|| "story segment ID is invalid".to_string())?;
                validate_segment_relations(project, id)?;
                let kind = object_string(segment, "kind")
                    .ok_or_else(|| "story segment kind is invalid".to_string())?;
                let start = segment
                    .pointer("/sourceRange/start")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| "story source range is invalid".to_string())?
                    as usize;
                let end = segment
                    .pointer("/sourceRange/end")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| "story source range is invalid".to_string())?
                    as usize;
                if kind == "transition" {
                    let seconds = segment.get("seconds").and_then(Value::as_u64).unwrap_or(0);
                    if index == 0
                        || index + 1 >= segments.len()
                        || object_string(&segments[index - 1], "kind") != Some("story")
                        || object_string(&segments[index + 1], "kind") != Some("story")
                        || start != cursor
                        || end != cursor
                        || !(2..=3).contains(&seconds)
                    {
                        return Err("story transition placement is invalid".to_string());
                    }
                } else if kind == "story" && start == cursor && end > cursor {
                    cursor = end;
                } else {
                    return Err("story source coverage is not contiguous".to_string());
                }
            }
            if cursor != source_length {
                return Err("story segments do not cover the complete source".to_string());
            }
        }
        StoryDesignStage::Refine => {
            for target in &state.target_ids {
                validate_segment_relations(project, target)?;
                let segment = segments
                    .iter()
                    .find(|value| object_string(value, "id") == Some(target))
                    .ok_or_else(|| "story refine target is absent".to_string())?;
                let detail = segment
                    .get("detail")
                    .filter(|value| !value.is_null())
                    .ok_or_else(|| "story refine target has no visual plan".to_string())?;
                validate_detail(
                    detail,
                    segment
                        .get("seconds")
                        .and_then(Value::as_i64)
                        .unwrap_or_default(),
                )?;
            }
        }
    }
    Ok(())
}

fn validate_project_shape(project: &Map<String, Value>) -> Result<(), String> {
    let title = string_field(project, "title")?;
    let description = string_field(project, "description")?;
    let source = string_field(project, "source")?;
    let style = string_field(project, "style")?;
    let ratio = string_field(project, "ratio")?;
    let summary = string_field(project, "summary")?;
    if title.trim().is_empty()
        || character_count(title) > 120
        || character_count(description) > 4_000
        || character_count(source) > 80_000
        || character_count(style) > 2_000
        || ratio.trim().is_empty()
        || character_count(summary) > 16_000
    {
        return Err("story project text fields are invalid".to_string());
    }
    let models = project
        .get("models")
        .and_then(Value::as_object)
        .ok_or_else(|| "story model selection is invalid".to_string())?;
    for field in ["textModelID", "imageModelID", "videoModelID"] {
        if string_field(models, field)?.trim().is_empty() {
            return Err("story model selection is invalid".to_string());
        }
    }
    let characters = array_field(project, "characters")?;
    let scenes = array_field(project, "scenes")?;
    let props = array_field(project, "props")?;
    let segments = array_field(project, "segments")?;
    let relations = array_field(project, "relations")?;
    if characters.len() > 100
        || scenes.len() > 100
        || props.len() > 100
        || segments.len() > 200
        || relations.len() > 1_600
    {
        return Err("story project collection limit is exceeded".to_string());
    }

    let mut ids = HashSet::new();
    for (collection, values) in [
        ("characters", characters),
        ("scenes", scenes),
        ("props", props),
    ] {
        for value in values {
            let object = value
                .as_object()
                .ok_or_else(|| "story resource must be an object".to_string())?;
            let id = string_field(object, "id")?;
            let name = string_field(object, "name")?;
            let image_prompt = string_field(object, "imagePrompt")?;
            if id.trim().is_empty()
                || id.trim() != id
                || character_count(id) > 128
                || name.trim().is_empty()
                || character_count(name) > 120
                || image_prompt.trim().is_empty()
                || character_count(image_prompt) > 4_000
                || !ids.insert(id)
            {
                return Err("story resource identity or text is invalid".to_string());
            }
            validate_media(object.get("media"))?;
            if collection == "characters" {
                validate_profile(
                    object.get("profile"),
                    &[
                        "roleInStory",
                        "appearance",
                        "personality",
                        "motivation",
                        "relationships",
                        "costume",
                        "consistencyNotes",
                    ],
                )?;
                if object
                    .get("profile")
                    .and_then(Value::as_object)
                    .and_then(|profile| profile.get("isProtagonist"))
                    .and_then(Value::as_bool)
                    .is_none()
                {
                    return Err("story character protagonist flag is invalid".to_string());
                }
            } else if collection == "scenes" {
                validate_profile(
                    object.get("profile"),
                    &[
                        "roleInStory",
                        "setting",
                        "spatialLayout",
                        "lightingAndPalette",
                        "keyElements",
                        "atmosphere",
                        "consistencyNotes",
                    ],
                )?;
            } else {
                let description = string_field(object, "description")?;
                if description.trim().is_empty() || character_count(description) > 2_000 {
                    return Err("story prop description is invalid".to_string());
                }
            }
        }
    }

    let character_ids = characters
        .iter()
        .filter_map(|value| object_string(value, "id"))
        .collect::<HashSet<_>>();
    let scene_ids = scenes
        .iter()
        .filter_map(|value| object_string(value, "id"))
        .collect::<HashSet<_>>();
    let prop_ids = props
        .iter()
        .filter_map(|value| object_string(value, "id"))
        .collect::<HashSet<_>>();
    let mut segment_ids = HashSet::new();
    for (index, value) in segments.iter().enumerate() {
        let object = value
            .as_object()
            .ok_or_else(|| "story segment must be an object".to_string())?;
        let id = string_field(object, "id")?;
        let title = string_field(object, "title")?;
        let synopsis = string_field(object, "synopsis")?;
        let kind = string_field(object, "kind")?;
        let seconds = integer_field(object, "seconds")?;
        let range = object
            .get("sourceRange")
            .and_then(Value::as_object)
            .ok_or_else(|| "story segment source range is invalid".to_string())?;
        let start = integer_field(range, "start")?;
        let end = integer_field(range, "end")?;
        let referenced = validate_id_references(object, "characterIDs", &character_ids)?
            + validate_id_references(object, "sceneIDs", &scene_ids)?
            + validate_id_references(object, "propIDs", &prop_ids)?;
        if id.trim().is_empty()
            || id.trim() != id
            || character_count(id) > 128
            || title.trim().is_empty()
            || character_count(title) > 120
            || character_count(synopsis) > 2_000
            || !(2..=15).contains(&seconds)
            || referenced > 8
            || start < 0
            || end < start
            || end as usize > character_count(source)
            || !segment_ids.insert(id)
        {
            return Err("story segment identity or bounds are invalid".to_string());
        }
        if kind == "transition" {
            if index == 0
                || index + 1 >= segments.len()
                || object_string(&segments[index - 1], "kind") != Some("story")
                || object_string(&segments[index + 1], "kind") != Some("story")
                || start != end
                || seconds > 3
            {
                return Err("story transition is invalid".to_string());
            }
        } else if kind != "story" || end <= start {
            return Err("story segment kind or source range is invalid".to_string());
        }
        validate_media(object.get("firstFrames"))?;
        validate_media(object.get("lastFrames"))?;
        if let Some(detail) = object.get("detail").filter(|value| !value.is_null()) {
            validate_detail(detail, seconds)?;
        }
    }

    let mut relation_ids = HashSet::new();
    for value in relations {
        let object = value
            .as_object()
            .ok_or_else(|| "story relation must be an object".to_string())?;
        let id = string_field(object, "id")?;
        let segment_id = string_field(object, "segmentID")?;
        let character_id = string_field(object, "characterID")?;
        let scene_id = string_field(object, "sceneID")?;
        let start = integer_field(object, "startSecond")?;
        let end = integer_field(object, "endSecond")?;
        let segment = segments
            .iter()
            .find(|value| object_string(value, "id") == Some(segment_id))
            .and_then(Value::as_object)
            .ok_or_else(|| "story relation segment is invalid".to_string())?;
        let duration = integer_field(segment, "seconds")?;
        if id.trim().is_empty()
            || character_count(id) > 128
            || !relation_ids.insert(id)
            || !array_field(segment, "characterIDs")?
                .iter()
                .any(|value| value.as_str() == Some(character_id))
            || !array_field(segment, "sceneIDs")?
                .iter()
                .any(|value| value.as_str() == Some(scene_id))
            || string_field(object, "action")?.trim().is_empty()
            || character_count(string_field(object, "action")?) > 400
            || string_field(object, "position")?.trim().is_empty()
            || character_count(string_field(object, "position")?) > 400
            || start < 0
            || end <= start
            || end > duration
        {
            return Err("story relation is invalid".to_string());
        }
    }
    for (index, value) in relations.iter().enumerate() {
        for other in relations.iter().skip(index + 1) {
            if object_string(value, "segmentID") == object_string(other, "segmentID")
                && object_string(value, "characterID") == object_string(other, "characterID")
                && object_string(value, "sceneID") != object_string(other, "sceneID")
                && relation_intervals_overlap(value, other)?
            {
                return Err("story character occupies overlapping scenes".to_string());
            }
        }
    }
    Ok(())
}

fn validate_profile(value: Option<&Value>, fields: &[&str]) -> Result<(), String> {
    let profile = value
        .and_then(Value::as_object)
        .ok_or_else(|| "story visual profile is invalid".to_string())?;
    for field in fields {
        let value = string_field(profile, field)?;
        if value.trim().is_empty() || character_count(value) > 600 {
            return Err(format!("story visual profile field {field} is invalid"));
        }
    }
    Ok(())
}

fn validate_id_references(
    object: &Map<String, Value>,
    field: &str,
    authorized: &HashSet<&str>,
) -> Result<usize, String> {
    let values = array_field(object, field)?;
    let mut unique = HashSet::new();
    for value in values {
        let id = value
            .as_str()
            .ok_or_else(|| format!("story segment {field} contains a non-string ID"))?;
        if !authorized.contains(id) || !unique.insert(id) {
            return Err(format!("story segment {field} contains an invalid ID"));
        }
    }
    Ok(values.len())
}

fn validate_media(value: Option<&Value>) -> Result<(), String> {
    let media = value
        .and_then(Value::as_object)
        .ok_or_else(|| "story media collection is invalid".to_string())?;
    let images = array_field(media, "images")?;
    let mut ids = HashSet::new();
    for image in images {
        let id =
            object_string(image, "id").ok_or_else(|| "story image ID is invalid".to_string())?;
        if !ids.insert(id) {
            return Err("story image IDs collide".to_string());
        }
    }
    if let Some(confirmed) = media
        .get("confirmedImageID")
        .filter(|value| !value.is_null())
    {
        let confirmed = confirmed
            .as_str()
            .ok_or_else(|| "story confirmed image ID is invalid".to_string())?;
        if !ids.contains(confirmed) {
            return Err("story confirmed image is absent".to_string());
        }
    }
    Ok(())
}

fn relation_intervals_overlap(left: &Value, right: &Value) -> Result<bool, String> {
    let left = left
        .as_object()
        .ok_or_else(|| "story relation must be an object".to_string())?;
    let right = right
        .as_object()
        .ok_or_else(|| "story relation must be an object".to_string())?;
    Ok(
        integer_field(left, "startSecond")?.max(integer_field(right, "startSecond")?)
            < integer_field(left, "endSecond")?.min(integer_field(right, "endSecond")?),
    )
}

fn validate_segment_relations(
    project: &Map<String, Value>,
    segment_id: &str,
) -> Result<(), String> {
    let segment = array_field(project, "segments")?
        .iter()
        .find(|value| object_string(value, "id") == Some(segment_id))
        .and_then(Value::as_object)
        .ok_or_else(|| "story segment is absent".to_string())?;
    let characters = array_field(segment, "characterIDs")?;
    let scenes = array_field(segment, "sceneIDs")?;
    if scenes.is_empty() {
        return Err("story segment requires a scene".to_string());
    }
    let relations = array_field(project, "relations")?
        .iter()
        .filter(|value| object_string(value, "segmentID") == Some(segment_id))
        .collect::<Vec<_>>();
    for character in characters.iter().filter_map(Value::as_str) {
        if !relations
            .iter()
            .any(|relation| object_string(relation, "characterID") == Some(character))
        {
            return Err("story character has no scene relation".to_string());
        }
    }
    Ok(())
}

fn validate_detail(detail: &Value, duration: i64) -> Result<(), String> {
    let object = detail
        .as_object()
        .ok_or_else(|| "story detail is invalid".to_string())?;
    if !(2..=15).contains(&duration) || string_field(object, "firstFramePrompt")?.trim().is_empty()
    {
        return Err("story first frame or duration is invalid".to_string());
    }
    let shots = array_field(object, "shots")?;
    if shots.is_empty() || shots.len() > 8 {
        return Err("story shots are invalid".to_string());
    }
    let mut cursor = 0;
    for shot in shots {
        let shot = shot
            .as_object()
            .ok_or_else(|| "story shot is invalid".to_string())?;
        let start = integer_field(shot, "start")?;
        let end = integer_field(shot, "end")?;
        if start != cursor
            || end <= start
            || end > duration
            || string_field(shot, "prompt")?.trim().is_empty()
        {
            return Err("story shot timeline is not contiguous".to_string());
        }
        cursor = end;
    }
    if cursor != duration {
        return Err("story shots do not cover the segment duration".to_string());
    }
    Ok(())
}

fn require_stage(state: &StoryDesignState, expected: StoryDesignStage) -> Result<(), String> {
    if state.stage != expected {
        return Err("story tool is not authorized for this planning stage".to_string());
    }
    Ok(())
}

fn read_result(value: Value) -> StoryToolResult {
    StoryToolResult {
        bounded_result: value,
        changed: false,
    }
}

fn write_result(message: impl Into<String>, changed: bool) -> StoryToolResult {
    StoryToolResult {
        bounded_result: json!({"message": message.into()}),
        changed,
    }
}

fn empty_media() -> Value {
    json!({
        "images": [],
        "confirmedImageID": null,
        "generationAttemptID": null
    })
}

fn resource_index(value: &Value, kind: &str) -> Value {
    json!({
        "id": value.get("id"),
        "name": value.get("name"),
        "kind": kind
    })
}

fn preview(value: Option<&Value>, maximum: usize) -> String {
    value
        .and_then(Value::as_str)
        .map(|value| character_slice(value, 0, maximum))
        .unwrap_or_default()
}

fn character_count(value: &str) -> usize {
    value.chars().count()
}

fn character_slice(value: &str, offset: usize, limit: usize) -> String {
    value.chars().skip(offset).take(limit).collect()
}

fn project_object(value: &Value) -> Result<&Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| "story draft must be a project object".to_string())
}

fn project_object_mut(value: &mut Value) -> Result<&mut Map<String, Value>, String> {
    value
        .as_object_mut()
        .ok_or_else(|| "story draft must be a project object".to_string())
}

fn string_field<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a str, String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("story field {field} must be a string"))
}

fn integer_field(object: &Map<String, Value>, field: &str) -> Result<i64, String> {
    object
        .get(field)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("story field {field} must be an integer"))
}

fn array_field<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a Vec<Value>, String> {
    object
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("story field {field} must be an array"))
}

fn array_field_mut<'a>(
    object: &'a mut Map<String, Value>,
    field: &str,
) -> Result<&'a mut Vec<Value>, String> {
    object
        .get_mut(field)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("story field {field} must be an array"))
}

fn object_string<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}

fn argument_object(arguments: &Value) -> Result<&Map<String, Value>, String> {
    arguments
        .as_object()
        .ok_or_else(|| "story tool arguments must be an object".to_string())
}

fn string_argument<'a>(arguments: &'a Value, field: &str) -> Result<&'a str, String> {
    let value = string_field(argument_object(arguments)?, field)?;
    if value.trim().is_empty() || value.trim() != value {
        return Err(format!("story argument {field} is invalid"));
    }
    Ok(value)
}

fn usize_argument(
    arguments: &Value,
    field: &str,
    minimum: usize,
    maximum: usize,
) -> Result<usize, String> {
    let value = argument_object(arguments)?
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("story argument {field} must be a non-negative integer"))?;
    if !(minimum..=maximum).contains(&value) {
        return Err(format!(
            "story argument {field} is outside its allowed range"
        ));
    }
    Ok(value)
}

fn string_array_argument(
    arguments: &Value,
    field: &str,
    maximum: usize,
) -> Result<Vec<String>, String> {
    let values = argument_object(arguments)?
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("story argument {field} must be an array"))?;
    if values.len() > maximum {
        return Err(format!("story argument {field} contains too many values"));
    }
    let mut result = Vec::with_capacity(values.len());
    let mut unique = HashSet::new();
    for value in values {
        let value = value
            .as_str()
            .filter(|value| !value.trim().is_empty() && value.trim() == *value)
            .ok_or_else(|| format!("story argument {field} contains an invalid ID"))?;
        if !unique.insert(value) {
            return Err(format!("story argument {field} contains duplicate IDs"));
        }
        result.push(value.to_string());
    }
    Ok(result)
}

fn find_asset<'a>(project: &'a Map<String, Value>, id: &str) -> Option<(&'static str, &'a Value)> {
    for (collection, kind) in [
        ("characters", "character"),
        ("scenes", "scene"),
        ("props", "prop"),
    ] {
        if let Some(value) = project
            .get(collection)
            .and_then(Value::as_array)
            .and_then(|values| {
                values
                    .iter()
                    .find(|value| object_string(value, "id") == Some(id))
            })
        {
            return Some((kind, value));
        }
    }
    None
}

fn reject_cross_kind_collision(
    project: &Map<String, Value>,
    id: &str,
    authorized_collection: &str,
) -> Result<(), String> {
    for collection in ["characters", "scenes", "props"] {
        if collection != authorized_collection
            && array_field(project, collection)?
                .iter()
                .any(|value| object_string(value, "id") == Some(id))
        {
            return Err("story resource ID is already used by another kind".to_string());
        }
    }
    Ok(())
}

fn join_profile(profile: &Value, fields: &[&str]) -> Result<String, String> {
    let object = profile
        .as_object()
        .ok_or_else(|| "story profile must be an object".to_string())?;
    let mut parts = Vec::with_capacity(fields.len());
    for field in fields {
        let value = string_field(object, field)?;
        if value.trim().is_empty() || character_count(value) > 600 {
            return Err(format!("story profile field {field} is invalid"));
        }
        parts.push(value);
    }
    Ok(parts.join("\n"))
}

fn upsert_value(
    values: &mut Vec<Value>,
    id: &str,
    value: Value,
    message: &str,
) -> Result<StoryToolResult, String> {
    if let Some(index) = values
        .iter()
        .position(|existing| object_string(existing, "id") == Some(id))
    {
        if values[index] == value {
            return Ok(write_result(message, false));
        }
        if values[index]
            .pointer("/media/images")
            .and_then(Value::as_array)
            .is_some_and(|images| !images.is_empty())
        {
            return Err("story resource with generated media is immutable".to_string());
        }
        values[index] = value;
    } else {
        values.push(value);
    }
    Ok(write_result(message, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_catalogs_expose_only_their_authorized_draft_tools() {
        let outline = story_tool_definitions(StoryDesignStage::Outline);
        let refine = story_tool_definitions(StoryDesignStage::Refine);

        assert_eq!(outline.len(), 14);
        assert_eq!(refine.len(), 10);
        assert!(outline
            .iter()
            .any(|tool| tool.name == "story_save_character_profile"));
        assert!(!outline
            .iter()
            .any(|tool| tool.name == "story_update_segment"));
        assert!(refine
            .iter()
            .any(|tool| tool.name == "story_update_segment"));
        assert!(!refine
            .iter()
            .any(|tool| tool.name == "story_append_segments"));
        assert!(
            outline
                .iter()
                .filter(|tool| tool.name.starts_with("story_save_")
                    || tool.name == "story_upsert_asset")
                .all(|tool| tool.effect == ToolEffect::DraftWrite)
        );
        assert_eq!(
            outline.last().map(|tool| (tool.name, tool.effect)),
            Some((STORY_FINISH_TOOL, ToolEffect::Terminal))
        );
    }

    #[test]
    fn outline_is_built_incrementally_and_finishes_only_when_complete() {
        let mut state = outline_state();
        assert!(validate_story_completion(&state).is_err());

        execute_story_tool(
            &mut state,
            "story_read_source",
            &json!({"offset":0,"limit":4}),
        )
        .unwrap();
        execute_story_tool(
            &mut state,
            "story_save_summary",
            &json!({"summary":"A hero returns home."}),
        )
        .unwrap();
        execute_story_tool(
            &mut state,
            "story_save_character_profile",
            &json!({
                "id":"hero","name":"Hero","profile":{
                    "isProtagonist":true,"roleInStory":"Returning protagonist",
                    "appearance":"Short dark hair","personality":"Observant",
                    "motivation":"Find the truth","relationships":"Lives alone",
                    "costume":"Dark wool coat","consistencyNotes":"Keep the same coat"
                }
            }),
        )
        .unwrap();
        execute_story_tool(
            &mut state,
            "story_save_scene_profile",
            &json!({
                "id":"room","name":"Old room","profile":{
                    "roleInStory":"Homecoming location","setting":"Old apartment at dusk",
                    "spatialLayout":"Door opposite a single window",
                    "lightingAndPalette":"Blue dusk and warm lamp",
                    "keyElements":"Wood table and dusty mirror","atmosphere":"Quiet tension",
                    "consistencyNotes":"Door and window stay opposite"
                }
            }),
        )
        .unwrap();
        execute_story_tool(
            &mut state,
            "story_append_segments",
            &json!({"segments":[{
                "id":"s1","title":"Return","synopsis":"The hero enters.",
                "kind":"story","seconds":4,"sourceStart":0,"sourceEnd":4
            }]}),
        )
        .unwrap();
        execute_story_tool(
            &mut state,
            "story_save_segment_relations",
            &json!({
                "segmentID":"s1","characterIDs":["hero"],"sceneIDs":["room"],"propIDs":[],
                "relations":[{
                    "relationID":"r1","characterID":"hero","sceneID":"room",
                    "action":"enters","position":"at the door","startSecond":0,"endSecond":4
                }]
            }),
        )
        .unwrap();

        validate_story_completion(&state).unwrap();
        assert_eq!(state.read_through, 4);
        assert_eq!(state.draft["segments"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn refine_requires_a_durable_read_before_writing_the_visual_plan() {
        let mut state = completed_outline_state();
        state.stage = StoryDesignStage::Refine;
        state.target_ids = vec!["s1".to_string()];
        let detail = json!({
            "segmentID":"s1","detail":{
                "firstFramePrompt":"Wide shot at the doorway",
                "lastFramePrompt":"The hero reaches the table",
                "shots":[{"start":0,"end":4,"prompt":"Slow dolly toward the hero"}],
                "continuityIn":"Begin outside the door","continuityOut":"End by the table",
                "audio":"Quiet room tone","constraints":"Preserve coat and room layout"
            }
        });

        let unchanged = state.clone();
        assert!(
            execute_story_tool(&mut state, "story_update_segment", &detail)
                .unwrap_err()
                .contains("must be read")
        );
        assert_eq!(state, unchanged);

        let read = execute_story_tool(&mut state, "story_read_segment", &json!({"segmentID":"s1"}))
            .unwrap();
        assert!(read.changed);
        execute_story_tool(&mut state, "story_update_segment", &detail).unwrap();
        validate_story_completion(&state).unwrap();
    }

    #[test]
    fn failed_draft_write_is_atomic() {
        let mut state = completed_outline_state();
        let original = state.clone();
        let error = execute_story_tool(
            &mut state,
            "story_save_segment_relations",
            &json!({
                "segmentID":"s1","characterIDs":["hero"],"sceneIDs":["missing"],"propIDs":[],
                "relations":[{
                    "relationID":"r2","characterID":"hero","sceneID":"missing",
                    "action":"waits","position":"center","startSecond":0,"endSecond":4
                }]
            }),
        )
        .unwrap_err();
        assert!(error.contains("invalid ID"));
        assert_eq!(state, original);
    }

    fn outline_state() -> StoryDesignState {
        StoryDesignState {
            schema_version: 1,
            story_record_id: "story-record-1".to_string(),
            project_id: "project-1".to_string(),
            base_project_revision: 7,
            base_project_digest: "digest-1".to_string(),
            stage: StoryDesignStage::Outline,
            target_ids: Vec::new(),
            draft: json!({
                "id":"project-1","version":2,"title":"Homecoming","description":"A visual short.",
                "models":{"textModelID":"text-1","imageModelID":"image-1","videoModelID":"video-1"},
                "source":"归来旧屋","style":"Natural cinematic light","ratio":"16:9","summary":"",
                "characters":[],"scenes":[],"props":[],"segments":[],"relations":[],
                "createdAt":"2026-09-14T00:00:00Z","updatedAt":"2026-09-14T00:00:00Z"
            }),
            read_through: 0,
            read_segment_ids: Vec::new(),
        }
    }

    fn completed_outline_state() -> StoryDesignState {
        let mut state = outline_state();
        execute_story_tool(
            &mut state,
            "story_read_source",
            &json!({"offset":0,"limit":4}),
        )
        .unwrap();
        execute_story_tool(
            &mut state,
            "story_save_summary",
            &json!({"summary":"A hero returns home."}),
        )
        .unwrap();
        execute_story_tool(
            &mut state,
            "story_save_character_profile",
            &json!({
                "id":"hero","name":"Hero","profile":{
                    "isProtagonist":true,"roleInStory":"Returning protagonist",
                    "appearance":"Short dark hair","personality":"Observant",
                    "motivation":"Find the truth","relationships":"Lives alone",
                    "costume":"Dark wool coat","consistencyNotes":"Keep the same coat"
                }
            }),
        )
        .unwrap();
        execute_story_tool(
            &mut state,
            "story_save_scene_profile",
            &json!({
                "id":"room","name":"Old room","profile":{
                    "roleInStory":"Homecoming location","setting":"Old apartment at dusk",
                    "spatialLayout":"Door opposite a single window",
                    "lightingAndPalette":"Blue dusk and warm lamp",
                    "keyElements":"Wood table and dusty mirror","atmosphere":"Quiet tension",
                    "consistencyNotes":"Door and window stay opposite"
                }
            }),
        )
        .unwrap();
        execute_story_tool(
            &mut state,
            "story_append_segments",
            &json!({"segments":[{
                "id":"s1","title":"Return","synopsis":"The hero enters.",
                "kind":"story","seconds":4,"sourceStart":0,"sourceEnd":4
            }]}),
        )
        .unwrap();
        execute_story_tool(
            &mut state,
            "story_save_segment_relations",
            &json!({
                "segmentID":"s1","characterIDs":["hero"],"sceneIDs":["room"],"propIDs":[],
                "relations":[{
                    "relationID":"r1","characterID":"hero","sceneID":"room",
                    "action":"enters","position":"at the door","startSecond":0,"endSecond":4
                }]
            }),
        )
        .unwrap();
        state
    }
}
