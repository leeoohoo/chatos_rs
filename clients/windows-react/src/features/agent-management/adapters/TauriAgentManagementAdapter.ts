import { invoke } from "@tauri-apps/api/core";
import {
  agentProfileEditorCatalogSchema,
  agentProfileEditorDataSchema,
  agentProfileListSchema,
  agentProfileSummarySchema,
  agentAvatarDraftSchema,
  agentConversationActivitySnapshotSchema,
  agentProfileActivitySnapshotSchema,
  agentTeamListSchema,
  agentTeamDetailSchema,
  agentTeamConversationSnapshotSchema,
  agentTeamMemberActivitySnapshotSchema,
  queuedAgentTeamMessageSchema,
  dispatchedAgentTeamDeliverySchema,
  agentTeamRunHistorySnapshotSchema,
  agentProjectDashboardSnapshotSchema,
  agentTeamTodoListSchema,
  agentTeamTodoProgressListSchema,
  agentTeamTodoSchema,
  agentTeamAssetListSchema,
  agentTeamAssetRevisionListSchema,
  agentTeamAssetSchema,
  agentTeamSummarySchema,
  preparedAgentConversationSchema,
  agentStaffingProposalListSchema,
  agentStaffingResolutionSchema,
  type AgentStaffingProposal,
  type AgentProfileMutationInput,
  type CreateAgentTeamInput,
  type UpdateAgentTeamMemberInput,
  type SaveAgentTeamAssetInput,
  type SaveAgentTeamTodoInput,
} from "../domain/agentManagement";
import { z } from "zod";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { mediaStudioHistoryPageSchema } from "../../media-studio/domain/mediaStudio";
import {
  agentSkillCatalogSchema,
  agentSkillDefinitionSchema,
  type AgentSkillKind,
  type AgentSkillMutationInput,
} from "../domain/agentSkillManagement";
import {
  requirementSurveyListSchema,
  requirementSurveySchema,
  type RequirementSurveyAnswers,
} from "../../requirement-surveys/domain/requirementSurvey";
import {
  agentArtifactPageSchema,
  agentArtifactPreviewSchema,
} from "../domain/agentArtifact";

export class TauriAgentManagementAdapter implements AgentManagementPort {
  async listProfiles(ownerUserId: string) {
    return agentProfileListSchema.parse(
      await invoke("list_agent_profiles", { ownerUserId }),
    );
  }

  async loadEditorCatalog(ownerUserId: string) {
    return agentProfileEditorCatalogSchema.parse(
      await invoke("load_agent_profile_editor_catalog", { ownerUserId }),
    );
  }

  async loadProfileEditor(ownerUserId: string, agentId: string) {
    return agentProfileEditorDataSchema.parse(
      await invoke("load_agent_profile_editor", { ownerUserId, agentId }),
    );
  }

  async saveProfile(ownerUserId: string, input: AgentProfileMutationInput) {
    return agentProfileSummarySchema.parse(
      await invoke("save_agent_profile", { ownerUserId, input }),
    );
  }

  async pickAvatar(ownerUserId: string, presentationId: string) {
    return agentAvatarDraftSchema.nullable().parse(
      await invoke("pick_agent_avatar", { input: { ownerUserId, presentationId } }),
    );
  }

  async loadAvatarHistory(ownerUserId: string) {
    const page = mediaStudioHistoryPageSchema.parse(
      await invoke("load_media_studio_image_history", { ownerUserId, after: null, limit: 40 }),
    );
    return page.items.flatMap((item) => item.images.flatMap((asset) => asset.previewUrl ? [{
      recordId: item.id,
      contentToken: asset.contentToken,
      previewUrl: asset.previewUrl,
      createdAtUnixMs: item.createdAtUnixMs,
    }] : []));
  }

  async discardAvatarReferences(ownerUserId: string, presentationId: string, referenceIds: string[]) {
    await invoke("discard_media_studio_reference_images", {
      input: { ownerUserId, presentationId, referenceIds },
    });
  }

  async discardAvatarPresentation(ownerUserId: string, presentationId: string) {
    await invoke("discard_media_studio_presentation", { input: { ownerUserId, presentationId } });
  }

  async listArtifacts(ownerUserId: string, limit: number, cursor: string | null) {
    return agentArtifactPageSchema.parse(
      await invoke("list_agent_artifacts", { ownerUserId, limit, cursor }),
    );
  }

  async previewArtifact(
    ownerUserId: string,
    artifactId: string,
    expectedSize: number,
    expectedSha256: string,
  ) {
    return agentArtifactPreviewSchema.parse(await invoke("preview_agent_artifact", {
      ownerUserId, artifactId, expectedSize, expectedSha256,
    }));
  }

  async saveArtifact(
    ownerUserId: string,
    artifactId: string,
    name: string,
    expectedSize: number,
    expectedSha256: string,
  ) {
    return z.boolean().parse(await invoke("save_agent_artifact", {
      ownerUserId, artifactId, name, expectedSize, expectedSha256,
    }));
  }

  async loadSkillCatalog(ownerUserId: string) {
    return agentSkillCatalogSchema.parse(
      await invoke("load_agent_skill_catalog", { ownerUserId }),
    );
  }

  async saveSkillOverride(ownerUserId: string, input: AgentSkillMutationInput) {
    return agentSkillDefinitionSchema.parse(
      await invoke("save_agent_skill_override", { ownerUserId, input }),
    );
  }

  async resetSkillOverride(ownerUserId: string, kind: AgentSkillKind, key: string) {
    return agentSkillDefinitionSchema.parse(
      await invoke("reset_agent_skill_override", { ownerUserId, kind, key }),
    );
  }

  async prepareConversation(ownerUserId: string, agentId: string) {
    return preparedAgentConversationSchema.parse(
      await invoke("prepare_agent_conversation", { ownerUserId, agentId }),
    );
  }

  async loadConversationActivity(ownerUserId: string, roomId: string) {
    return agentConversationActivitySnapshotSchema.parse(
      await invoke("load_agent_conversation_activity", { ownerUserId, roomId }),
    );
  }

  async loadProfileActivity(ownerUserId: string, agentId: string) {
    return agentProfileActivitySnapshotSchema.parse(
      await invoke("load_agent_profile_activity", { ownerUserId, agentId }),
    );
  }

  async listTeams(ownerUserId: string) {
    return agentTeamListSchema.parse(
      await invoke("list_agent_teams", { ownerUserId }),
    );
  }

  async createTeam(ownerUserId: string, input: CreateAgentTeamInput) {
    return agentTeamSummarySchema.parse(
      await invoke("create_agent_team", { input: { ownerUserId, ...input } }),
    );
  }

  async loadTeam(ownerUserId: string, teamId: string) {
    return agentTeamDetailSchema.parse(
      await invoke("load_agent_team", { ownerUserId, teamId }),
    );
  }

  async inviteAgentToTeam(ownerUserId: string, teamId: string, agentId: string) {
    return agentTeamDetailSchema.parse(
      await invoke("invite_agent_to_team", { input: { ownerUserId, teamId, agentId } }),
    );
  }

  async updateTeamMember(
    ownerUserId: string,
    teamId: string,
    agentId: string,
    input: UpdateAgentTeamMemberInput,
  ) {
    return agentTeamDetailSchema.parse(
      await invoke("update_agent_team_member", {
        input: { ownerUserId, teamId, agentId, ...input },
      }),
    );
  }

  async setTeamDefaultAgent(ownerUserId: string, teamId: string, agentId: string) {
    return agentTeamDetailSchema.parse(
      await invoke("set_agent_team_default_agent", { input: { ownerUserId, teamId, agentId } }),
    );
  }

  async setTeamProjectManager(ownerUserId: string, teamId: string, agentId: string) {
    return agentTeamDetailSchema.parse(
      await invoke("set_agent_team_project_manager", { input: { ownerUserId, teamId, agentId } }),
    );
  }

  async removeAgentFromTeam(ownerUserId: string, teamId: string, agentId: string) {
    return agentTeamDetailSchema.parse(
      await invoke("remove_agent_from_team", { input: { ownerUserId, teamId, agentId } }),
    );
  }

  async listStaffingProposals(ownerUserId: string, sourceRoomId: string) {
    return agentStaffingProposalListSchema.parse(
      await invoke("list_agent_staffing_proposals", { ownerUserId, sourceRoomId }),
    );
  }

  async resolveStaffingProposal(
    ownerUserId: string,
    sourceRoomId: string,
    proposalId: string,
    kind: AgentStaffingProposal["kind"],
    approved: boolean,
  ) {
    return agentStaffingResolutionSchema.parse(
      await invoke("resolve_agent_staffing_proposal", {
        input: { ownerUserId, sourceRoomId, proposalId, kind, approved },
      }),
    );
  }

  async loadTeamConversation(
    ownerUserId: string,
    teamId: string,
    beforeMessageId: string | null = null,
    limit = 50,
  ) {
    return agentTeamConversationSnapshotSchema.parse(
      await invoke("load_agent_team_conversation", { ownerUserId, teamId, beforeMessageId, limit }),
    );
  }

  async queueTeamMessage(
    ownerUserId: string,
    teamId: string,
    content: string,
    mentionedAgentIds: string[],
    attachmentIds: string[],
  ) {
    return queuedAgentTeamMessageSchema.parse(
      await invoke("queue_agent_team_message", {
        input: { ownerUserId, teamId, content, mentionedAgentIds, attachmentIds },
      }),
    );
  }

  async dispatchNextTeamDelivery(ownerUserId: string, teamId: string) {
    return dispatchedAgentTeamDeliverySchema.nullable().parse(
      await invoke("dispatch_next_agent_team_delivery", { ownerUserId, teamId }),
    );
  }

  async loadTeamMemberActivity(ownerUserId: string, teamId: string, agentId: string) {
    return agentTeamMemberActivitySnapshotSchema.parse(
      await invoke("load_agent_team_member_activity", { ownerUserId, teamId, agentId }),
    );
  }

  async loadTeamRuns(ownerUserId: string, teamId: string) {
    return agentTeamRunHistorySnapshotSchema.parse(
      await invoke("load_agent_team_runs", { ownerUserId, teamId }),
    );
  }

  async loadTeamDashboard(ownerUserId: string, teamId: string) {
    return agentProjectDashboardSnapshotSchema.parse(
      await invoke("load_agent_team_dashboard", { ownerUserId, teamId }),
    );
  }

  async retryTeamRun(ownerUserId: string, teamId: string, runId: string) {
    return agentTeamTodoSchema.parse(
      await invoke("retry_agent_team_run", { input: { ownerUserId, teamId, runId } }),
    );
  }

  async listTeamTodos(ownerUserId: string, teamId: string, includeTerminal: boolean) {
    return agentTeamTodoListSchema.parse(
      await invoke("list_agent_team_todos", { ownerUserId, teamId, includeTerminal }),
    );
  }

  async listTeamTodoProgress(ownerUserId: string, teamId: string, todoId: string, limit = 100) {
    return agentTeamTodoProgressListSchema.parse(
      await invoke("list_agent_team_todo_progress", { ownerUserId, teamId, todoId, limit }),
    );
  }

  async createTeamTodo(ownerUserId: string, teamId: string, input: SaveAgentTeamTodoInput) {
    return agentTeamTodoSchema.parse(
      await invoke("create_agent_team_todo", {
        input: {
          ownerUserId, teamId, agentId: input.agentId, title: input.title,
          detail: input.detail, priority: input.priority, dependencyIds: input.dependencyIds,
          executionContract: null,
        },
      }),
    );
  }

  async updateTeamTodo(ownerUserId: string, teamId: string, input: SaveAgentTeamTodoInput) {
    if (!input.todoId || input.expectedUpdatedAtUnixMs == null) {
      throw new Error("团队任务更新缺少版本信息。");
    }
    return agentTeamTodoSchema.parse(
      await invoke("update_agent_team_todo", {
        input: {
          ownerUserId, teamId, todoId: input.todoId,
          expectedUpdatedAtUnixMs: input.expectedUpdatedAtUnixMs,
          title: input.title, detail: input.detail, priority: input.priority,
          dependencyIds: input.dependencyIds, executionContract: null,
        },
      }),
    );
  }

  async reorderTeamTodos(ownerUserId: string, teamId: string, todoIds: string[]) {
    return agentTeamTodoListSchema.parse(
      await invoke("reorder_agent_team_todos", { input: { ownerUserId, teamId, todoIds } }),
    );
  }

  async cancelTeamTodo(
    ownerUserId: string,
    teamId: string,
    todoId: string,
    expectedUpdatedAtUnixMs: number,
  ) {
    return agentTeamTodoSchema.parse(
      await invoke("cancel_agent_team_todo", {
        input: { ownerUserId, teamId, todoId, expectedUpdatedAtUnixMs },
      }),
    );
  }

  async listTeamAssets(ownerUserId: string, teamId: string) {
    return agentTeamAssetListSchema.parse(
      await invoke("list_agent_team_assets", { ownerUserId, teamId }),
    );
  }

  async saveTeamAsset(ownerUserId: string, teamId: string, input: SaveAgentTeamAssetInput) {
    return agentTeamAssetSchema.parse(
      await invoke("save_agent_team_asset", { input: { ownerUserId, teamId, ...input } }),
    );
  }

  async archiveTeamAsset(
    ownerUserId: string,
    teamId: string,
    assetId: string,
    expectedRevision: number,
  ) {
    return agentTeamAssetSchema.parse(
      await invoke("archive_agent_team_asset", {
        input: { ownerUserId, teamId, assetId, expectedRevision },
      }),
    );
  }

  async listTeamAssetRevisions(ownerUserId: string, teamId: string, assetId: string) {
    return agentTeamAssetRevisionListSchema.parse(
      await invoke("list_agent_team_asset_revisions", { ownerUserId, teamId, assetId }),
    );
  }

  async listProjectRequirementSurveys(ownerUserId: string, projectId: string) {
    return requirementSurveyListSchema.parse(
      await invoke("list_project_requirement_surveys", { ownerUserId, projectId }),
    );
  }

  async resolveProjectRequirementSurvey(
    ownerUserId: string,
    projectId: string,
    surveyId: string,
    expectedVersion: number,
    answers: RequirementSurveyAnswers,
  ) {
    return requirementSurveySchema.parse(
      await invoke("resolve_project_requirement_survey", {
        input: { ownerUserId, projectId, surveyId, expectedVersion, answers },
      }),
    );
  }

  async sendConversationDraft(input: Parameters<NonNullable<AgentManagementPort["sendConversationDraft"]>>[0]) {
    return z.object({
      deliveryKind: z.enum(["guidance", "newTurn"]),
      turnId: z.string().min(1),
      userMessageId: z.string().min(1),
    }).strict().parse(await invoke("send_agent_conversation_draft", { input }));
  }
}
