import type {
  AgentProfileEditorCatalog,
  AgentProfileEditorData,
  AgentProfileMutationInput,
  AgentProfileSummary,
  AgentAvatarDraft,
  AgentAvatarHistoryAsset,
  AgentConversationActivitySnapshot,
  AgentProfileActivitySnapshot,
  AgentTeamSummary,
  AgentTeamDetail,
  AgentTeamConversationSnapshot,
  AgentTeamMemberActivitySnapshot,
  QueuedAgentTeamMessage,
  DispatchedAgentTeamDelivery,
  AgentTeamRunHistorySnapshot,
  AgentTeamTodo,
  AgentTeamTodoProgress,
  AgentTeamAsset,
  AgentTeamAssetRevision,
  SaveAgentTeamAssetInput,
  SaveAgentTeamTodoInput,
  ProjectRequirementSurvey,
  ProjectRequirementSurveyAnswers,
  CreateAgentTeamInput,
  UpdateAgentTeamMemberInput,
  PreparedAgentConversation,
  AgentStaffingProposal,
  AgentStaffingResolution,
} from "../domain/agentManagement";
import type { ConversationSendAck } from "../../conversation-composer/domain/conversationComposer";
import type {
  AgentSkillDefinition,
  AgentSkillKind,
  AgentSkillMutationInput,
} from "../domain/agentSkillManagement";

export interface AgentManagementPort {
  listProfiles(ownerUserId: string): Promise<AgentProfileSummary[]>;
  loadEditorCatalog(ownerUserId: string): Promise<AgentProfileEditorCatalog>;
  loadProfileEditor(ownerUserId: string, agentId: string): Promise<AgentProfileEditorData>;
  saveProfile(ownerUserId: string, input: AgentProfileMutationInput): Promise<AgentProfileSummary>;
  pickAvatar(ownerUserId: string, presentationId: string): Promise<AgentAvatarDraft | null>;
  loadAvatarHistory(ownerUserId: string): Promise<AgentAvatarHistoryAsset[]>;
  discardAvatarReferences(ownerUserId: string, presentationId: string, referenceIds: string[]): Promise<void>;
  discardAvatarPresentation(ownerUserId: string, presentationId: string): Promise<void>;
  loadSkillCatalog?(ownerUserId: string): Promise<AgentSkillDefinition[]>;
  saveSkillOverride?(ownerUserId: string, input: AgentSkillMutationInput): Promise<AgentSkillDefinition>;
  resetSkillOverride?(ownerUserId: string, kind: AgentSkillKind, key: string): Promise<AgentSkillDefinition>;
  prepareConversation?(ownerUserId: string, agentId: string): Promise<PreparedAgentConversation>;
  loadConversationActivity?(
    ownerUserId: string,
    roomId: string,
  ): Promise<AgentConversationActivitySnapshot>;
  loadProfileActivity?(
    ownerUserId: string,
    agentId: string,
  ): Promise<AgentProfileActivitySnapshot>;
  listTeams?(ownerUserId: string): Promise<AgentTeamSummary[]>;
  createTeam?(ownerUserId: string, input: CreateAgentTeamInput): Promise<AgentTeamSummary>;
  loadTeam?(ownerUserId: string, teamId: string): Promise<AgentTeamDetail>;
  inviteAgentToTeam?(ownerUserId: string, teamId: string, agentId: string): Promise<AgentTeamDetail>;
  updateTeamMember?(
    ownerUserId: string,
    teamId: string,
    agentId: string,
    input: UpdateAgentTeamMemberInput,
  ): Promise<AgentTeamDetail>;
  setTeamDefaultAgent?(ownerUserId: string, teamId: string, agentId: string): Promise<AgentTeamDetail>;
  setTeamProjectManager?(ownerUserId: string, teamId: string, agentId: string): Promise<AgentTeamDetail>;
  removeAgentFromTeam?(ownerUserId: string, teamId: string, agentId: string): Promise<AgentTeamDetail>;
  listStaffingProposals?(ownerUserId: string, sourceRoomId: string): Promise<AgentStaffingProposal[]>;
  resolveStaffingProposal?(
    ownerUserId: string,
    sourceRoomId: string,
    proposalId: string,
    kind: AgentStaffingProposal["kind"],
    approved: boolean,
  ): Promise<AgentStaffingResolution>;
  loadTeamConversation?(ownerUserId: string, teamId: string): Promise<AgentTeamConversationSnapshot>;
  queueTeamMessage?(
    ownerUserId: string,
    teamId: string,
    content: string,
    mentionedAgentIds: string[],
    attachmentIds: string[],
  ): Promise<QueuedAgentTeamMessage>;
  dispatchNextTeamDelivery?(
    ownerUserId: string,
    teamId: string,
  ): Promise<DispatchedAgentTeamDelivery | null>;
  loadTeamMemberActivity?(
    ownerUserId: string,
    teamId: string,
    agentId: string,
  ): Promise<AgentTeamMemberActivitySnapshot>;
  loadTeamRuns?(ownerUserId: string, teamId: string): Promise<AgentTeamRunHistorySnapshot>;
  retryTeamRun?(
    ownerUserId: string,
    teamId: string,
    runId: string,
  ): Promise<AgentTeamTodo>;
  listTeamTodos?(ownerUserId: string, teamId: string, includeTerminal: boolean): Promise<AgentTeamTodo[]>;
  listTeamTodoProgress?(
    ownerUserId: string,
    teamId: string,
    todoId: string,
    limit?: number,
  ): Promise<AgentTeamTodoProgress[]>;
  createTeamTodo?(
    ownerUserId: string,
    teamId: string,
    input: SaveAgentTeamTodoInput,
  ): Promise<AgentTeamTodo>;
  updateTeamTodo?(
    ownerUserId: string,
    teamId: string,
    input: SaveAgentTeamTodoInput,
  ): Promise<AgentTeamTodo>;
  reorderTeamTodos?(ownerUserId: string, teamId: string, todoIds: string[]): Promise<AgentTeamTodo[]>;
  cancelTeamTodo?(
    ownerUserId: string,
    teamId: string,
    todoId: string,
    expectedUpdatedAtUnixMs: number,
  ): Promise<AgentTeamTodo>;
  listTeamAssets?(ownerUserId: string, teamId: string): Promise<AgentTeamAsset[]>;
  saveTeamAsset?(
    ownerUserId: string,
    teamId: string,
    input: SaveAgentTeamAssetInput,
  ): Promise<AgentTeamAsset>;
  archiveTeamAsset?(
    ownerUserId: string,
    teamId: string,
    assetId: string,
    expectedRevision: number,
  ): Promise<AgentTeamAsset>;
  listTeamAssetRevisions?(
    ownerUserId: string,
    teamId: string,
    assetId: string,
  ): Promise<AgentTeamAssetRevision[]>;
  listProjectRequirementSurveys?(
    ownerUserId: string,
    projectId: string,
  ): Promise<ProjectRequirementSurvey[]>;
  resolveProjectRequirementSurvey?(
    ownerUserId: string,
    projectId: string,
    surveyId: string,
    expectedVersion: number,
    answers: ProjectRequirementSurveyAnswers,
  ): Promise<ProjectRequirementSurvey>;
  sendConversationDraft?(input: {
    ownerUserId: string;
    roomId: string;
    guidanceTurnId: string | null;
    submission: { content: string; attachmentIds: string[] };
  }): Promise<ConversationSendAck>;
}
