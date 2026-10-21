use super::team_todo::{resolve_agent_reference, todo_planning_reference};
use super::*;
use rusqlite::Transaction;
use serde_json::Value;

const STAFF_HIRE: &str = "agent.staff.hire";
const STAFF_TERMINATE: &str = "agent.staff.terminate";
const LEGACY_PROJECT_STEWARD: &str = "builtin.project-steward";

const PROFESSION_KEYS: &[&str] = &[
    "project_manager",
    "product_manager",
    "technical_manager",
    "solution_architect",
    "security_engineer",
    "qa_engineer",
    "software_engineer",
    "fullstack_engineer",
    "frontend_engineer",
    "backend_engineer",
    "mobile_engineer",
    "desktop_engineer",
    "game_engineer",
    "embedded_iot_engineer",
    "database_engineer",
    "devops_engineer",
    "data_engineer",
    "data_analyst",
    "machine_learning_engineer",
    "research_specialist",
    "product_designer",
    "ui_designer",
    "ux_designer",
    "game_designer",
    "technical_writer",
    "business_analyst",
    "implementation_consultant",
    "erp_consultant",
    "wms_consultant",
    "domain_expert",
    "operations_specialist",
    "growth_marketing_specialist",
    "general_member",
];

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentCreationProposalDraft {
    pub(crate) name: String,
    pub(crate) role: String,
    #[serde(default)]
    pub(crate) responsibility: String,
    pub(crate) role_prompt: String,
    pub(crate) model_config_id: String,
    pub(crate) thinking_level: Option<String>,
    pub(crate) profession_key: String,
    #[serde(default)]
    pub(crate) rationale: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentMembershipProposalDraft {
    pub(crate) target_team_room_id: String,
    pub(crate) target_agent_id: String,
    pub(crate) role: String,
    #[serde(default)]
    pub(crate) responsibility: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AgentRemovalProposalDraft {
    pub(crate) target_agent_id: String,
    pub(crate) reason: String,
    #[serde(default)]
    pub(crate) handoff_plan: String,
}

#[derive(Clone, Debug)]
struct ProposalRunBinding {
    communication: AgentCommunicationToolBinding,
    delivery_id: String,
    model_config_id: String,
    thinking_level: Option<String>,
    default_skill_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentStaffingProposal {
    pub(super) id: String,
    pub(super) kind: String,
    source_room_id: String,
    proposer_agent_id: String,
    status: String,
    name: Option<String>,
    role: Option<String>,
    responsibility: Option<String>,
    role_prompt: Option<String>,
    thinking_level: Option<String>,
    profession_key: Option<String>,
    rationale: Option<String>,
    target_team_id: Option<String>,
    target_agent_id: Option<String>,
    reason: Option<String>,
    handoff_plan: Option<String>,
    created_at_unix_ms: i64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentStaffingResolution {
    proposal_id: String,
    kind: String,
    pub(super) status: String,
    source_room_id: String,
    affected_team_id: Option<String>,
    pub(super) created_agent_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ResolveAgentStaffingProposalInput {
    pub(crate) owner_user_id: String,
    pub(crate) source_room_id: String,
    pub(crate) proposal_id: String,
    pub(crate) kind: String,
    pub(crate) approved: bool,
}

impl AgentConversationStore {
    pub(crate) fn create_agent_creation_proposal(
        &self,
        owner_user_id: &str,
        run_id: &str,
        request_key: &str,
        name: &str,
        role: &str,
        responsibility: &str,
        role_prompt: &str,
        thinking_level: Option<&str>,
        profession_key: &str,
        rationale: &str,
        now_unix_ms: i64,
    ) -> Result<(AgentCommunicationToolBinding, Value), String> {
        validate_creation_fields(
            name,
            role,
            responsibility,
            role_prompt,
            thinking_level,
            profession_key,
            rationale,
        )?;
        validate_proposal_request(run_id, request_key, now_unix_ms)?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| proposal_store_error())?;
        let binding = load_proposal_run_binding(&transaction, owner_user_id, run_id)?;
        require_staff_permission(&binding.default_skill_ids)?;
        let draft = AgentCreationProposalDraft {
            name: name.to_owned(),
            role: role.to_owned(),
            responsibility: responsibility.to_owned(),
            role_prompt: role_prompt.to_owned(),
            model_config_id: binding.model_config_id.clone(),
            thinking_level: thinking_level
                .map(str::to_owned)
                .or_else(|| binding.thinking_level.clone()),
            profession_key: profession_key.to_owned(),
            rationale: rationale.to_owned(),
        };
        validate_creation_draft(&draft)?;
        let draft_json = canonical_json(&draft)?;
        if let Some(existing) = existing_draft(
            &transaction,
            "local_agent_creation_proposals",
            "room_id",
            owner_user_id,
            &binding.communication.room_id,
            &binding.communication.agent_id,
            &binding.delivery_id,
            request_key,
        )? {
            if existing != draft_json {
                return Err("The proposal invocation ID was reused with different input.".into());
            }
            transaction.commit().map_err(|_| proposal_store_error())?;
            return Ok((
                binding.communication,
                proposal_acknowledgement("agent_creation", &draft.name),
            ));
        }
        transaction
            .execute(
                r#"
                INSERT INTO local_agent_creation_proposals (
                    owner_user_id, id, room_id, proposer_agent_id, source_delivery_id,
                    request_key, draft_json, status, created_agent_id,
                    created_at_unix_ms, resolved_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', NULL, ?8, NULL)
                "#,
                params![
                    owner_user_id,
                    identifier("proposal"),
                    binding.communication.room_id,
                    binding.communication.agent_id,
                    binding.delivery_id,
                    request_key,
                    draft_json,
                    now_unix_ms,
                ],
            )
            .map_err(|_| proposal_store_error())?;
        transaction.commit().map_err(|_| proposal_store_error())?;
        Ok((
            binding.communication,
            proposal_acknowledgement("agent_creation", &draft.name),
        ))
    }

    pub(crate) fn create_agent_membership_proposal(
        &self,
        owner_user_id: &str,
        run_id: &str,
        request_key: &str,
        team_reference: &str,
        target_agent_reference: &str,
        role: &str,
        responsibility: &str,
        now_unix_ms: i64,
    ) -> Result<(AgentCommunicationToolBinding, Value), String> {
        validate_text(role, 160, false).map_err(|_| proposal_input_error())?;
        validate_text(responsibility, 8_000, true).map_err(|_| proposal_input_error())?;
        validate_text(team_reference, 600, false).map_err(|_| proposal_input_error())?;
        validate_text(target_agent_reference, 600, false).map_err(|_| proposal_input_error())?;
        validate_proposal_request(run_id, request_key, now_unix_ms)?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| proposal_store_error())?;
        let binding = load_proposal_run_binding(&transaction, owner_user_id, run_id)?;
        require_staff_permission(&binding.default_skill_ids)?;
        let target_team_room_id =
            resolve_staffing_team_reference(&transaction, owner_user_id, run_id, team_reference)?;
        let target_agent_id =
            resolve_agent_reference(&transaction, owner_user_id, run_id, target_agent_reference)?;
        let already_member: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_agent_room_members WHERE owner_user_id = ?1 \
                 AND room_id = ?2 AND agent_id = ?3 AND status = 'active')",
                params![owner_user_id, target_team_room_id, target_agent_id],
                |row| row.get(0),
            )
            .map_err(|_| proposal_store_error())?;
        if already_member {
            return Err("The Agent is already an active member of the target team.".into());
        }
        let draft = AgentMembershipProposalDraft {
            target_team_room_id,
            target_agent_id,
            role: role.to_owned(),
            responsibility: responsibility.to_owned(),
        };
        let draft_json = canonical_json(&draft)?;
        if let Some(existing) = existing_draft(
            &transaction,
            "local_agent_membership_proposals",
            "source_room_id",
            owner_user_id,
            &binding.communication.room_id,
            &binding.communication.agent_id,
            &binding.delivery_id,
            request_key,
        )? {
            if existing != draft_json {
                return Err("The proposal invocation ID was reused with different input.".into());
            }
            transaction.commit().map_err(|_| proposal_store_error())?;
            return Ok((
                binding.communication,
                proposal_acknowledgement("existing_agent_membership", "现有 Agent 入队提案"),
            ));
        }
        transaction
            .execute(
                r#"
                INSERT INTO local_agent_membership_proposals (
                    owner_user_id, id, source_room_id, proposer_agent_id,
                    source_delivery_id, request_key, draft_json, status,
                    created_at_unix_ms, resolved_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8, NULL)
                "#,
                params![
                    owner_user_id,
                    identifier("proposal"),
                    binding.communication.room_id,
                    binding.communication.agent_id,
                    binding.delivery_id,
                    request_key,
                    draft_json,
                    now_unix_ms,
                ],
            )
            .map_err(|_| proposal_store_error())?;
        transaction.commit().map_err(|_| proposal_store_error())?;
        Ok((
            binding.communication,
            proposal_acknowledgement("existing_agent_membership", "现有 Agent 入队提案"),
        ))
    }

    pub(crate) fn create_agent_removal_proposal(
        &self,
        owner_user_id: &str,
        run_id: &str,
        request_key: &str,
        target_agent_reference: &str,
        reason: &str,
        handoff_plan: &str,
        now_unix_ms: i64,
    ) -> Result<(AgentCommunicationToolBinding, Value), String> {
        validate_text(target_agent_reference, 600, false).map_err(|_| proposal_input_error())?;
        validate_text(reason, 4_000, false).map_err(|_| proposal_input_error())?;
        validate_text(handoff_plan, 8_000, true).map_err(|_| proposal_input_error())?;
        validate_proposal_request(run_id, request_key, now_unix_ms)?;
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| proposal_store_error())?;
        let binding = load_proposal_run_binding(&transaction, owner_user_id, run_id)?;
        require_staff_permission(&binding.default_skill_ids)?;
        if binding.communication.conversation_kind != "project_team" {
            return Err("Member removal proposals are available only in project teams.".into());
        }
        let target_agent_id =
            resolve_agent_reference(&transaction, owner_user_id, run_id, target_agent_reference)?;
        if target_agent_id == binding.communication.agent_id {
            return Err("An Agent cannot propose removing itself from the current team.".into());
        }
        let active_member: bool = transaction
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_agent_room_members WHERE owner_user_id = ?1 \
                 AND room_id = ?2 AND agent_id = ?3 AND status = 'active')",
                params![
                    owner_user_id,
                    binding.communication.room_id,
                    target_agent_id
                ],
                |row| row.get(0),
            )
            .map_err(|_| proposal_store_error())?;
        if !active_member {
            return Err("The target Agent is not an active member of this team.".into());
        }
        let draft = AgentRemovalProposalDraft {
            target_agent_id,
            reason: reason.to_owned(),
            handoff_plan: handoff_plan.to_owned(),
        };
        let draft_json = canonical_json(&draft)?;
        if let Some(existing) = existing_draft(
            &transaction,
            "local_agent_removal_proposals",
            "room_id",
            owner_user_id,
            &binding.communication.room_id,
            &binding.communication.agent_id,
            &binding.delivery_id,
            request_key,
        )? {
            if existing != draft_json {
                return Err("The proposal invocation ID was reused with different input.".into());
            }
            transaction.commit().map_err(|_| proposal_store_error())?;
            return Ok((
                binding.communication,
                proposal_acknowledgement("agent_removal", "团队成员移出提案"),
            ));
        }
        transaction
            .execute(
                r#"
                INSERT INTO local_agent_removal_proposals (
                    owner_user_id, id, room_id, proposer_agent_id, source_delivery_id,
                    request_key, draft_json, status, created_at_unix_ms, resolved_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8, NULL)
                "#,
                params![
                    owner_user_id,
                    identifier("proposal"),
                    binding.communication.room_id,
                    binding.communication.agent_id,
                    binding.delivery_id,
                    request_key,
                    draft_json,
                    now_unix_ms,
                ],
            )
            .map_err(|_| proposal_store_error())?;
        transaction.commit().map_err(|_| proposal_store_error())?;
        Ok((
            binding.communication,
            proposal_acknowledgement("agent_removal", "团队成员移出提案"),
        ))
    }

    pub(crate) fn list_agent_staffing_proposals(
        &self,
        owner_user_id: &str,
        source_room_id: &str,
    ) -> Result<Vec<AgentStaffingProposal>, String> {
        validate_identifier(source_room_id).map_err(|_| proposal_input_error())?;
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        require_source_room(&connection, owner_user_id, source_room_id)?;
        let mut proposals = Vec::new();
        load_creation_proposals(&connection, owner_user_id, source_room_id, &mut proposals)?;
        load_membership_proposals(&connection, owner_user_id, source_room_id, &mut proposals)?;
        load_removal_proposals(&connection, owner_user_id, source_room_id, &mut proposals)?;
        proposals.sort_by(|left, right| {
            left.created_at_unix_ms
                .cmp(&right.created_at_unix_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        Ok(proposals)
    }

    pub(crate) fn pending_creation_model(
        &self,
        owner_user_id: &str,
        source_room_id: &str,
        proposal_id: &str,
    ) -> Result<(String, Option<String>), String> {
        reject_link_or_non_file(&self.database)?;
        let connection = open_database(&self.database, true)?;
        let draft_json = connection
            .query_row(
                "SELECT draft_json FROM local_agent_creation_proposals WHERE owner_user_id = ?1 \
                 AND room_id = ?2 AND id = ?3 AND status = 'pending'",
                params![owner_user_id, source_room_id, proposal_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| proposal_store_error())?
            .ok_or_else(proposal_conflict)?;
        let draft: AgentCreationProposalDraft =
            serde_json::from_str(&draft_json).map_err(|_| proposal_store_error())?;
        validate_creation_draft(&draft)?;
        Ok((draft.model_config_id, draft.thinking_level))
    }

    pub(crate) fn resolve_agent_staffing_proposal(
        &self,
        owner_user_id: &str,
        source_room_id: &str,
        proposal_id: &str,
        kind: &str,
        approved: bool,
        now_unix_ms: i64,
    ) -> Result<AgentStaffingResolution, String> {
        validate_identifier(source_room_id).map_err(|_| proposal_input_error())?;
        validate_identifier(proposal_id).map_err(|_| proposal_input_error())?;
        if now_unix_ms < 0 {
            return Err(proposal_input_error());
        }
        reject_link_or_non_file(&self.database)?;
        let mut connection = open_database(&self.database, false)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| proposal_store_error())?;
        require_source_room(&transaction, owner_user_id, source_room_id)?;
        let resolution = match kind {
            "agentCreation" => resolve_creation(
                &transaction,
                owner_user_id,
                source_room_id,
                proposal_id,
                approved,
                now_unix_ms,
            )?,
            "existingAgentMembership" => resolve_membership(
                &transaction,
                owner_user_id,
                source_room_id,
                proposal_id,
                approved,
                now_unix_ms,
            )?,
            "agentRemoval" => resolve_removal(
                &transaction,
                owner_user_id,
                source_room_id,
                proposal_id,
                approved,
                now_unix_ms,
            )?,
            _ => return Err(proposal_input_error()),
        };
        transaction.commit().map_err(|_| proposal_store_error())?;
        Ok(resolution)
    }
}

fn load_proposal_run_binding(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
) -> Result<ProposalRunBinding, String> {
    let communication = load_agent_communication_tool_binding(connection, owner_user_id, run_id)?;
    connection
        .query_row(
            r#"
            SELECT delivery.id, profile.model_config_id, profile.thinking_level,
                   profile.default_skill_ids_json
            FROM windows_agent_delivery_runs run
            JOIN project_agent_deliveries delivery
              ON delivery.owner_user_id = run.owner_user_id AND delivery.id = run.delivery_id
            JOIN local_agent_profiles profile
              ON profile.owner_user_id = delivery.owner_user_id
             AND profile.id = delivery.target_agent_id
            JOIN project_agent_rooms room
              ON room.owner_user_id = delivery.owner_user_id AND room.id = delivery.room_id
            WHERE run.owner_user_id = ?1 AND run.run_id = ?2
              AND delivery.trigger_kind != 'todo' AND profile.status = 'active'
              AND (
                (run.status IN ('ready', 'running') AND delivery.status = 'running')
                OR (
                  run.status = 'ready' AND delivery.status = 'pending'
                  AND delivery.trigger_kind = 'default_agent'
                  AND room.conversation_kind = 'human_agent_direct'
                )
              )
              AND delivery.room_id = ?3 AND delivery.target_agent_id = ?4
            "#,
            params![
                owner_user_id,
                run_id,
                communication.room_id,
                communication.agent_id
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|_| proposal_store_error())?
        .ok_or_else(|| "The Agent staffing Run binding is unavailable.".to_owned())
        .and_then(
            |(delivery_id, model_config_id, thinking_level, skill_ids_json)| {
                validate_identifier(&delivery_id).map_err(|_| proposal_store_error())?;
                validate_identifier(&model_config_id).map_err(|_| proposal_store_error())?;
                Ok(ProposalRunBinding {
                    communication,
                    delivery_id,
                    model_config_id,
                    thinking_level,
                    default_skill_ids: parse_identifier_array(&skill_ids_json)?,
                })
            },
        )
}

fn resolve_staffing_team_reference(
    connection: &Connection,
    owner_user_id: &str,
    run_id: &str,
    reference: &str,
) -> Result<String, String> {
    let mut statement = connection
        .prepare(
            "SELECT id FROM project_agent_rooms WHERE owner_user_id = ?1 AND status = 'active' \
             AND conversation_kind = 'project_team' ORDER BY id LIMIT 1000",
        )
        .map_err(|_| proposal_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id], |row| row.get::<_, String>(0))
        .map_err(|_| proposal_store_error())?;
    for row in rows {
        let team_id = row.map_err(|_| proposal_store_error())?;
        if todo_planning_reference(owner_user_id, run_id, "team", &team_id) == reference {
            return Ok(team_id);
        }
    }
    Err("The team reference is unavailable for this Run.".into())
}

fn existing_draft(
    transaction: &Transaction<'_>,
    table: &str,
    room_column: &str,
    owner_user_id: &str,
    room_id: &str,
    proposer_agent_id: &str,
    source_delivery_id: &str,
    request_key: &str,
) -> Result<Option<String>, String> {
    if !matches!(
        (table, room_column),
        ("local_agent_creation_proposals", "room_id")
            | ("local_agent_removal_proposals", "room_id")
            | ("local_agent_membership_proposals", "source_room_id")
    ) {
        return Err(proposal_store_error());
    }
    transaction
        .query_row(
            &format!(
                "SELECT draft_json FROM {table} WHERE owner_user_id = ?1 AND {room_column} = ?2 \
                 AND proposer_agent_id = ?3 AND source_delivery_id = ?4 AND request_key = ?5"
            ),
            params![
                owner_user_id,
                room_id,
                proposer_agent_id,
                source_delivery_id,
                request_key
            ],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| proposal_store_error())
}

fn resolve_creation(
    transaction: &Transaction<'_>,
    owner_user_id: &str,
    source_room_id: &str,
    proposal_id: &str,
    approved: bool,
    now_unix_ms: i64,
) -> Result<AgentStaffingResolution, String> {
    let (proposer_agent_id, draft_json, conversation_kind): (String, String, String) = transaction
        .query_row(
            r#"
            SELECT proposal.proposer_agent_id, proposal.draft_json, room.conversation_kind
            FROM local_agent_creation_proposals proposal
            JOIN project_agent_rooms room
              ON room.owner_user_id = proposal.owner_user_id AND room.id = proposal.room_id
            WHERE proposal.owner_user_id = ?1 AND proposal.room_id = ?2
              AND proposal.id = ?3 AND proposal.status = 'pending'
              AND room.status = 'active'
            "#,
            params![owner_user_id, source_room_id, proposal_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|_| proposal_store_error())?
        .ok_or_else(proposal_conflict)?;
    let draft: AgentCreationProposalDraft =
        serde_json::from_str(&draft_json).map_err(|_| proposal_store_error())?;
    validate_creation_draft(&draft)?;
    let mut created_agent_id = None;
    let mut affected_team_id = None;
    if approved {
        let agent_id = identifier("agent");
        transaction
            .execute(
                r#"
                INSERT INTO local_agent_profiles (
                    owner_user_id, id, name, description, role_prompt, model_config_id,
                    thinking_level, profession_key, default_plugin_ids_json,
                    default_skill_ids_json, heartbeat_enabled, heartbeat_interval_seconds,
                    heartbeat_prompt, last_heartbeat_at_unix_ms, next_heartbeat_at_unix_ms,
                    avatar_data, status, created_at_unix_ms, updated_at_unix_ms
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '[]', '[]', 0, 900, '',
                          NULL, NULL, NULL, 'active', ?9, ?9)
                "#,
                params![
                    owner_user_id,
                    agent_id,
                    draft.name,
                    draft.responsibility,
                    draft.role_prompt,
                    draft.model_config_id,
                    draft.thinking_level,
                    draft.profession_key,
                    now_unix_ms,
                ],
            )
            .map_err(|_| proposal_store_error())?;
        if conversation_kind == "project_team" {
            transaction
                .execute(
                    r#"
                    INSERT INTO project_agent_room_members (
                        owner_user_id, room_id, agent_id, role, responsibility,
                        plugin_allowlist_json, status, joined_at_unix_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, '[]', 'active', ?6)
                    "#,
                    params![
                        owner_user_id,
                        source_room_id,
                        agent_id,
                        draft.role,
                        draft.responsibility,
                        now_unix_ms,
                    ],
                )
                .map_err(|_| proposal_store_error())?;
            transaction
                .execute(
                    "UPDATE project_agent_rooms SET default_agent_id = COALESCE(default_agent_id, ?1), \
                     updated_at_unix_ms = max(updated_at_unix_ms, ?2) WHERE owner_user_id = ?3 \
                     AND id = ?4 AND status = 'active' AND conversation_kind = 'project_team'",
                    params![agent_id, now_unix_ms, owner_user_id, source_room_id],
                )
                .map_err(|_| proposal_store_error())?;
            affected_team_id = Some(source_room_id.to_owned());
        } else if !matches!(
            conversation_kind.as_str(),
            "human_agent_direct" | "agent_agent_direct"
        ) {
            return Err(proposal_conflict());
        }
        created_agent_id = Some(agent_id);
    }
    let status = if approved { "approved" } else { "rejected" };
    let changed = transaction
        .execute(
            "UPDATE local_agent_creation_proposals SET status = ?1, created_agent_id = ?2, \
             resolved_at_unix_ms = ?3 WHERE owner_user_id = ?4 AND room_id = ?5 AND id = ?6 \
             AND status = 'pending'",
            params![
                status,
                created_agent_id,
                now_unix_ms,
                owner_user_id,
                source_room_id,
                proposal_id
            ],
        )
        .map_err(|_| proposal_store_error())?;
    if changed != 1 {
        return Err(proposal_conflict());
    }
    enqueue_resolution_notification(
        transaction,
        owner_user_id,
        source_room_id,
        &proposer_agent_id,
        proposal_id,
        "新 Agent 创建提案",
        approved,
        now_unix_ms,
    )?;
    Ok(AgentStaffingResolution {
        proposal_id: proposal_id.to_owned(),
        kind: "agentCreation".into(),
        status: status.into(),
        source_room_id: source_room_id.to_owned(),
        affected_team_id,
        created_agent_id,
    })
}

fn resolve_membership(
    transaction: &Transaction<'_>,
    owner_user_id: &str,
    source_room_id: &str,
    proposal_id: &str,
    approved: bool,
    now_unix_ms: i64,
) -> Result<AgentStaffingResolution, String> {
    let (proposer_agent_id, draft_json): (String, String) = transaction
        .query_row(
            "SELECT proposer_agent_id, draft_json FROM local_agent_membership_proposals \
             WHERE owner_user_id = ?1 AND source_room_id = ?2 AND id = ?3 AND status = 'pending'",
            params![owner_user_id, source_room_id, proposal_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| proposal_store_error())?
        .ok_or_else(proposal_conflict)?;
    let draft: AgentMembershipProposalDraft =
        serde_json::from_str(&draft_json).map_err(|_| proposal_store_error())?;
    validate_membership_draft(&draft)?;
    if approved {
        let (status, profession): (Option<String>, String) = transaction
            .query_row(
                "SELECT member.status, profile.profession_key FROM local_agent_profiles profile \
                 LEFT JOIN project_agent_room_members member ON member.owner_user_id = profile.owner_user_id \
                   AND member.agent_id = profile.id AND member.room_id = ?2 \
                 WHERE profile.owner_user_id = ?1 AND profile.id = ?3 AND profile.status = 'active' \
                   AND EXISTS(SELECT 1 FROM project_agent_rooms room WHERE room.owner_user_id = ?1 \
                     AND room.id = ?2 AND room.status = 'active' AND room.conversation_kind = 'project_team')",
                params![owner_user_id, draft.target_team_room_id, draft.target_agent_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|_| proposal_store_error())?
            .ok_or_else(proposal_conflict)?;
        match status.as_deref() {
            Some("active") => return Err(proposal_conflict()),
            Some("removed") => {
                transaction
                    .execute(
                        "UPDATE project_agent_room_members SET role = ?1, responsibility = ?2, \
                         plugin_allowlist_json = '[]', status = 'active', joined_at_unix_ms = ?3 \
                         WHERE owner_user_id = ?4 AND room_id = ?5 AND agent_id = ?6 \
                           AND status = 'removed'",
                        params![
                            draft.role,
                            draft.responsibility,
                            now_unix_ms,
                            owner_user_id,
                            draft.target_team_room_id,
                            draft.target_agent_id
                        ],
                    )
                    .map_err(|_| proposal_store_error())?;
            }
            None => {
                transaction
                    .execute(
                        r#"
                        INSERT INTO project_agent_room_members (
                            owner_user_id, room_id, agent_id, role, responsibility,
                            plugin_allowlist_json, status, joined_at_unix_ms
                        ) VALUES (?1, ?2, ?3, ?4, ?5, '[]', 'active', ?6)
                        "#,
                        params![
                            owner_user_id,
                            draft.target_team_room_id,
                            draft.target_agent_id,
                            draft.role,
                            draft.responsibility,
                            now_unix_ms
                        ],
                    )
                    .map_err(|_| proposal_store_error())?;
            }
            _ => return Err(proposal_store_error()),
        }
        let assign_manager = profession == "project_manager";
        transaction
            .execute(
                "UPDATE project_agent_rooms SET default_agent_id = COALESCE(default_agent_id, ?1), \
                 project_manager_agent_id = CASE WHEN project_manager_agent_id IS NULL AND ?2 = 1 \
                   THEN ?1 ELSE project_manager_agent_id END, updated_at_unix_ms = max(updated_at_unix_ms, ?3) \
                 WHERE owner_user_id = ?4 AND id = ?5 AND status = 'active' \
                   AND conversation_kind = 'project_team'",
                params![
                    draft.target_agent_id,
                    i64::from(assign_manager),
                    now_unix_ms,
                    owner_user_id,
                    draft.target_team_room_id
                ],
            )
            .map_err(|_| proposal_store_error())?;
    }
    let status = if approved { "approved" } else { "rejected" };
    let changed = transaction
        .execute(
            "UPDATE local_agent_membership_proposals SET status = ?1, resolved_at_unix_ms = ?2 \
             WHERE owner_user_id = ?3 AND source_room_id = ?4 AND id = ?5 AND status = 'pending'",
            params![
                status,
                now_unix_ms,
                owner_user_id,
                source_room_id,
                proposal_id
            ],
        )
        .map_err(|_| proposal_store_error())?;
    if changed != 1 {
        return Err(proposal_conflict());
    }
    enqueue_resolution_notification(
        transaction,
        owner_user_id,
        source_room_id,
        &proposer_agent_id,
        proposal_id,
        "现有 Agent 加入团队提案",
        approved,
        now_unix_ms,
    )?;
    Ok(AgentStaffingResolution {
        proposal_id: proposal_id.to_owned(),
        kind: "existingAgentMembership".into(),
        status: status.into(),
        source_room_id: source_room_id.to_owned(),
        affected_team_id: approved.then_some(draft.target_team_room_id),
        created_agent_id: None,
    })
}

fn resolve_removal(
    transaction: &Transaction<'_>,
    owner_user_id: &str,
    source_room_id: &str,
    proposal_id: &str,
    approved: bool,
    now_unix_ms: i64,
) -> Result<AgentStaffingResolution, String> {
    let (proposer_agent_id, draft_json, default_agent_id, manager_agent_id): (
        String,
        String,
        Option<String>,
        Option<String>,
    ) = transaction
        .query_row(
            r#"
            SELECT proposal.proposer_agent_id, proposal.draft_json,
                   room.default_agent_id, room.project_manager_agent_id
            FROM local_agent_removal_proposals proposal
            JOIN project_agent_rooms room
              ON room.owner_user_id = proposal.owner_user_id AND room.id = proposal.room_id
            WHERE proposal.owner_user_id = ?1 AND proposal.room_id = ?2
              AND proposal.id = ?3 AND proposal.status = 'pending'
              AND room.status = 'active' AND room.conversation_kind = 'project_team'
            "#,
            params![owner_user_id, source_room_id, proposal_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|_| proposal_store_error())?
        .ok_or_else(proposal_conflict)?;
    let draft: AgentRemovalProposalDraft =
        serde_json::from_str(&draft_json).map_err(|_| proposal_store_error())?;
    validate_removal_draft(&draft)?;
    if approved {
        if manager_agent_id.as_deref() == Some(&draft.target_agent_id) {
            return Err("请先指定新的项目经理，再批准移出当前项目经理。".into());
        }
        transaction
            .execute(
                "UPDATE project_agent_deliveries SET status = CASE WHEN status = 'pending' \
                 THEN 'cancelled' ELSE 'failed' END, last_error = '成员已由 Human 确认移出当前团队。', \
                 completed_at_unix_ms = ?1 WHERE owner_user_id = ?2 AND room_id = ?3 \
                 AND target_agent_id = ?4 AND status IN ('pending', 'running')",
                params![now_unix_ms, owner_user_id, source_room_id, draft.target_agent_id],
            )
            .map_err(|_| proposal_store_error())?;
        let changed = transaction
            .execute(
                "UPDATE project_agent_room_members SET status = 'removed' WHERE owner_user_id = ?1 \
                 AND room_id = ?2 AND agent_id = ?3 AND status = 'active'",
                params![owner_user_id, source_room_id, draft.target_agent_id],
            )
            .map_err(|_| proposal_store_error())?;
        if changed != 1 {
            return Err(proposal_conflict());
        }
        if default_agent_id.as_deref() == Some(&draft.target_agent_id) {
            let replacement = transaction
                .query_row(
                    "SELECT agent_id FROM project_agent_room_members WHERE owner_user_id = ?1 \
                     AND room_id = ?2 AND status = 'active' ORDER BY joined_at_unix_ms, agent_id LIMIT 1",
                    params![owner_user_id, source_room_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|_| proposal_store_error())?;
            transaction
                .execute(
                    "UPDATE project_agent_rooms SET default_agent_id = ?1, updated_at_unix_ms = ?2 \
                     WHERE owner_user_id = ?3 AND id = ?4 AND status = 'active'",
                    params![replacement, now_unix_ms, owner_user_id, source_room_id],
                )
                .map_err(|_| proposal_store_error())?;
        }
    }
    let status = if approved { "approved" } else { "rejected" };
    let changed = transaction
        .execute(
            "UPDATE local_agent_removal_proposals SET status = ?1, resolved_at_unix_ms = ?2 \
             WHERE owner_user_id = ?3 AND room_id = ?4 AND id = ?5 AND status = 'pending'",
            params![
                status,
                now_unix_ms,
                owner_user_id,
                source_room_id,
                proposal_id
            ],
        )
        .map_err(|_| proposal_store_error())?;
    if changed != 1 {
        return Err(proposal_conflict());
    }
    enqueue_resolution_notification(
        transaction,
        owner_user_id,
        source_room_id,
        &proposer_agent_id,
        proposal_id,
        "Agent 移出团队提案",
        approved,
        now_unix_ms,
    )?;
    Ok(AgentStaffingResolution {
        proposal_id: proposal_id.to_owned(),
        kind: "agentRemoval".into(),
        status: status.into(),
        source_room_id: source_room_id.to_owned(),
        affected_team_id: approved.then_some(source_room_id.to_owned()),
        created_agent_id: None,
    })
}

fn enqueue_resolution_notification(
    transaction: &Transaction<'_>,
    owner_user_id: &str,
    source_room_id: &str,
    proposer_agent_id: &str,
    proposal_id: &str,
    label: &str,
    approved: bool,
    now_unix_ms: i64,
) -> Result<(), String> {
    let active: bool = transaction
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM project_agent_room_members member \
             JOIN project_agent_rooms room ON room.owner_user_id = member.owner_user_id \
               AND room.id = member.room_id WHERE member.owner_user_id = ?1 \
               AND member.room_id = ?2 AND member.agent_id = ?3 AND member.status = 'active' \
               AND room.status = 'active')",
            params![owner_user_id, source_room_id, proposer_agent_id],
            |row| row.get(0),
        )
        .map_err(|_| proposal_store_error())?;
    if !active {
        return Ok(());
    }
    let decision = if approved { "批准" } else { "拒绝" };
    let follow_up = if approved {
        "该决定已经生效。请重新读取工作区快照确认最新状态，在当前会话回复 Human 后继续处理需要跟进的事项；不要等待 Human 再次提醒。"
    } else {
        "请在当前会话确认你已知晓，不要再次提交相同提案；如需替代方案，先根据 Human 的最新要求调整。"
    };
    let content =
        format!("Human 已{decision}你提交的{label}（proposal_id: {proposal_id}）。{follow_up}");
    let message_id = identifier("message");
    transaction
        .execute(
            r#"
            INSERT INTO project_agent_messages (
                owner_user_id, id, room_id, sender_kind, sender_id, content,
                reply_to_message_id, source_run_id, causation_id, root_message_id,
                hop_count, created_at_unix_ms
            ) VALUES (?1, ?2, ?3, 'system', 'system', ?4, NULL, NULL, ?5, ?2, 0, ?6)
            "#,
            params![
                owner_user_id,
                message_id,
                source_room_id,
                content,
                proposal_id,
                now_unix_ms
            ],
        )
        .map_err(|_| proposal_store_error())?;
    transaction
        .execute(
            "INSERT INTO project_agent_message_mentions (owner_user_id, message_id, agent_id, position) \
             VALUES (?1, ?2, ?3, 0)",
            params![owner_user_id, message_id, proposer_agent_id],
        )
        .map_err(|_| proposal_store_error())?;
    transaction
        .execute(
            r#"
            INSERT INTO project_agent_deliveries (
                owner_user_id, id, room_id, message_id, root_message_id,
                target_agent_id, trigger_kind, status, attempt, hop_count,
                deduplication_key, response_message_id, last_error,
                claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
            ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, 'mention', 'pending', 0, 0,
                      ?6, NULL, NULL, NULL, NULL, ?7)
            "#,
            params![
                owner_user_id,
                identifier("delivery"),
                source_room_id,
                message_id,
                proposer_agent_id,
                format!("proposal-resolution:{proposal_id}:{proposer_agent_id}"),
                now_unix_ms
            ],
        )
        .map_err(|_| proposal_store_error())?;
    Ok(())
}

fn load_creation_proposals(
    connection: &Connection,
    owner_user_id: &str,
    room_id: &str,
    output: &mut Vec<AgentStaffingProposal>,
) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT id, proposer_agent_id, draft_json, status, created_at_unix_ms \
             FROM local_agent_creation_proposals WHERE owner_user_id = ?1 AND room_id = ?2 \
             AND status = 'pending' ORDER BY created_at_unix_ms, id LIMIT 1000",
        )
        .map_err(|_| proposal_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id, room_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|_| proposal_store_error())?;
    for row in rows {
        let (id, proposer, draft_json, status, created_at) =
            row.map_err(|_| proposal_store_error())?;
        let draft: AgentCreationProposalDraft =
            serde_json::from_str(&draft_json).map_err(|_| proposal_store_error())?;
        validate_creation_draft(&draft)?;
        output.push(AgentStaffingProposal {
            id,
            kind: "agentCreation".into(),
            source_room_id: room_id.to_owned(),
            proposer_agent_id: proposer,
            status,
            name: Some(draft.name),
            role: Some(draft.role),
            responsibility: Some(draft.responsibility),
            role_prompt: Some(draft.role_prompt),
            thinking_level: draft.thinking_level,
            profession_key: Some(draft.profession_key),
            rationale: Some(draft.rationale),
            target_team_id: None,
            target_agent_id: None,
            reason: None,
            handoff_plan: None,
            created_at_unix_ms: created_at,
        });
    }
    Ok(())
}

fn load_membership_proposals(
    connection: &Connection,
    owner_user_id: &str,
    room_id: &str,
    output: &mut Vec<AgentStaffingProposal>,
) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT id, proposer_agent_id, draft_json, status, created_at_unix_ms \
             FROM local_agent_membership_proposals WHERE owner_user_id = ?1 AND source_room_id = ?2 \
             AND status = 'pending' ORDER BY created_at_unix_ms, id LIMIT 1000",
        )
        .map_err(|_| proposal_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id, room_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|_| proposal_store_error())?;
    for row in rows {
        let (id, proposer, draft_json, status, created_at) =
            row.map_err(|_| proposal_store_error())?;
        let draft: AgentMembershipProposalDraft =
            serde_json::from_str(&draft_json).map_err(|_| proposal_store_error())?;
        validate_membership_draft(&draft)?;
        output.push(AgentStaffingProposal {
            id,
            kind: "existingAgentMembership".into(),
            source_room_id: room_id.to_owned(),
            proposer_agent_id: proposer,
            status,
            name: None,
            role: Some(draft.role),
            responsibility: Some(draft.responsibility),
            role_prompt: None,
            thinking_level: None,
            profession_key: None,
            rationale: None,
            target_team_id: Some(draft.target_team_room_id),
            target_agent_id: Some(draft.target_agent_id),
            reason: None,
            handoff_plan: None,
            created_at_unix_ms: created_at,
        });
    }
    Ok(())
}

fn load_removal_proposals(
    connection: &Connection,
    owner_user_id: &str,
    room_id: &str,
    output: &mut Vec<AgentStaffingProposal>,
) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT id, proposer_agent_id, draft_json, status, created_at_unix_ms \
             FROM local_agent_removal_proposals WHERE owner_user_id = ?1 AND room_id = ?2 \
             AND status = 'pending' ORDER BY created_at_unix_ms, id LIMIT 1000",
        )
        .map_err(|_| proposal_store_error())?;
    let rows = statement
        .query_map(params![owner_user_id, room_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|_| proposal_store_error())?;
    for row in rows {
        let (id, proposer, draft_json, status, created_at) =
            row.map_err(|_| proposal_store_error())?;
        let draft: AgentRemovalProposalDraft =
            serde_json::from_str(&draft_json).map_err(|_| proposal_store_error())?;
        validate_removal_draft(&draft)?;
        output.push(AgentStaffingProposal {
            id,
            kind: "agentRemoval".into(),
            source_room_id: room_id.to_owned(),
            proposer_agent_id: proposer,
            status,
            name: None,
            role: None,
            responsibility: None,
            role_prompt: None,
            thinking_level: None,
            profession_key: None,
            rationale: None,
            target_team_id: Some(room_id.to_owned()),
            target_agent_id: Some(draft.target_agent_id),
            reason: Some(draft.reason),
            handoff_plan: Some(draft.handoff_plan),
            created_at_unix_ms: created_at,
        });
    }
    Ok(())
}

fn require_source_room(
    connection: &Connection,
    owner_user_id: &str,
    source_room_id: &str,
) -> Result<(), String> {
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM project_agent_rooms WHERE owner_user_id = ?1 \
             AND id = ?2 AND status = 'active')",
            params![owner_user_id, source_room_id],
            |row| row.get(0),
        )
        .map_err(|_| proposal_store_error())?;
    if !exists {
        return Err("提案来源会话不存在或已停用。".into());
    }
    Ok(())
}

fn require_staff_permission(skill_ids: &[String]) -> Result<(), String> {
    let values = skill_ids
        .iter()
        .map(String::as_str)
        .collect::<std::collections::HashSet<_>>();
    if values.contains(LEGACY_PROJECT_STEWARD)
        || (values.contains(STAFF_HIRE) && values.contains(STAFF_TERMINATE))
    {
        Ok(())
    } else {
        Err("The current Agent does not have staff-management permission.".into())
    }
}

fn validate_proposal_request(
    run_id: &str,
    request_key: &str,
    now_unix_ms: i64,
) -> Result<(), String> {
    validate_identifier(run_id).map_err(|_| proposal_input_error())?;
    validate_text(request_key, 512, false).map_err(|_| proposal_input_error())?;
    if now_unix_ms < 0 {
        return Err(proposal_input_error());
    }
    Ok(())
}

fn validate_creation_fields(
    name: &str,
    role: &str,
    responsibility: &str,
    role_prompt: &str,
    thinking_level: Option<&str>,
    profession_key: &str,
    rationale: &str,
) -> Result<(), String> {
    validate_text(name, 120, false).map_err(|_| proposal_input_error())?;
    validate_text(role, 160, false).map_err(|_| proposal_input_error())?;
    validate_text(responsibility, 8_000, true).map_err(|_| proposal_input_error())?;
    validate_text(role_prompt, 32_000, false).map_err(|_| proposal_input_error())?;
    validate_text(rationale, 4_000, true).map_err(|_| proposal_input_error())?;
    if !PROFESSION_KEYS.contains(&profession_key)
        || thinking_level.is_some_and(|level| {
            !matches!(
                level,
                "auto" | "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
            )
        })
    {
        return Err(proposal_input_error());
    }
    Ok(())
}

fn validate_creation_draft(draft: &AgentCreationProposalDraft) -> Result<(), String> {
    validate_creation_fields(
        &draft.name,
        &draft.role,
        &draft.responsibility,
        &draft.role_prompt,
        draft.thinking_level.as_deref(),
        &draft.profession_key,
        &draft.rationale,
    )?;
    validate_identifier(&draft.model_config_id).map_err(|_| proposal_store_error())
}

fn validate_membership_draft(draft: &AgentMembershipProposalDraft) -> Result<(), String> {
    validate_identifier(&draft.target_team_room_id).map_err(|_| proposal_store_error())?;
    validate_agent_identifier(&draft.target_agent_id).map_err(|_| proposal_store_error())?;
    validate_text(&draft.role, 160, false).map_err(|_| proposal_store_error())?;
    validate_text(&draft.responsibility, 8_000, true).map_err(|_| proposal_store_error())
}

fn validate_removal_draft(draft: &AgentRemovalProposalDraft) -> Result<(), String> {
    validate_agent_identifier(&draft.target_agent_id).map_err(|_| proposal_store_error())?;
    validate_text(&draft.reason, 4_000, false).map_err(|_| proposal_store_error())?;
    validate_text(&draft.handoff_plan, 8_000, true).map_err(|_| proposal_store_error())
}

fn canonical_json<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string(value).map_err(|_| proposal_store_error())
}

fn proposal_acknowledgement(kind: &str, subject: &str) -> Value {
    json!({"type": kind, "status": "pending", "subject": subject})
}

fn proposal_input_error() -> String {
    "The Agent staffing proposal input is invalid.".into()
}

fn proposal_conflict() -> String {
    "该提案已处理、已失效，或当前团队状态已发生变化。".into()
}

fn proposal_store_error() -> String {
    "Agent 人员提案数据不可用，请检查本机存储后重试。".into()
}

#[tauri::command]
pub(crate) async fn list_agent_staffing_proposals(
    owner_user_id: String,
    source_room_id: String,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
) -> Result<Vec<AgentStaffingProposal>, String> {
    validate_owner(&owner_user_id)?;
    validate_identifier(&source_room_id).map_err(|_| proposal_input_error())?;
    let generation = authentication
        .session_generation_for_owner(&owner_user_id)
        .await?;
    let store = conversations.inner().clone();
    let owner = owner_user_id.clone();
    let room = source_room_id.clone();
    let proposals = tauri::async_runtime::spawn_blocking(move || {
        store.list_agent_staffing_proposals(&owner, &room)
    })
    .await
    .map_err(|_| "Agent 人员提案读取任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&owner_user_id, generation)
        .await?;
    Ok(proposals)
}

#[tauri::command]
pub(crate) async fn resolve_agent_staffing_proposal(
    input: ResolveAgentStaffingProposalInput,
    app: AppHandle,
    authentication: State<'_, AuthenticationManager>,
    conversations: State<'_, AgentConversationStore>,
    attachment_worker: State<'_, AttachmentToolWorker>,
) -> Result<AgentStaffingResolution, String> {
    validate_owner(&input.owner_user_id)?;
    validate_identifier(&input.source_room_id).map_err(|_| proposal_input_error())?;
    validate_identifier(&input.proposal_id).map_err(|_| proposal_input_error())?;
    let generation = authentication
        .session_generation_for_owner(&input.owner_user_id)
        .await?;
    if input.approved && input.kind == "agentCreation" {
        let store = conversations.inner().clone();
        let owner = input.owner_user_id.clone();
        let room = input.source_room_id.clone();
        let proposal = input.proposal_id.clone();
        let (model_config_id, thinking_level) = tauri::async_runtime::spawn_blocking(move || {
            store.pending_creation_model(&owner, &room, &proposal)
        })
        .await
        .map_err(|_| "Agent 提案模型复验任务异常退出。".to_owned())??;
        let models = authentication
            .model_catalog_for_owner(&input.owner_user_id)
            .await
            .map_err(|error| error.message)?;
        let selected = models
            .iter()
            .find(|model| model.id == model_config_id && model.enabled && model.has_api_key)
            .ok_or_else(|| "提案继承的 Agent 模型当前不可用，请刷新模型配置后重试。".to_owned())?;
        if thinking_level.as_deref().is_some_and(|level| {
            level != "auto"
                && (!selected.supports_reasoning
                    || chatos_ai_runtime::model_config::normalize_thinking_level(
                        &selected.provider,
                        Some(level),
                    )
                    .is_err())
        }) {
            return Err("提案中的思考等级不再受当前模型支持。".into());
        }
    }
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    let store = conversations.inner().clone();
    let owner = input.owner_user_id.clone();
    let room = input.source_room_id.clone();
    let proposal = input.proposal_id.clone();
    let kind = input.kind.clone();
    let approved = input.approved;
    let resolution = tauri::async_runtime::spawn_blocking(move || {
        store.resolve_agent_staffing_proposal(
            &owner,
            &room,
            &proposal,
            &kind,
            approved,
            chrono::Utc::now().timestamp_millis().max(0),
        )
    })
    .await
    .map_err(|_| "Agent 人员提案审批任务异常退出。".to_owned())??;
    authentication
        .require_session_generation(&input.owner_user_id, generation)
        .await?;
    crate::attachment_tool_worker::schedule_agent_communication_drain(
        app,
        input.owner_user_id,
        resolution.source_room_id.clone(),
    );
    attachment_worker.wake();
    Ok(resolution)
}
