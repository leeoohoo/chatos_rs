import type {
  AgentProfileEditorCatalog,
  AgentProfileEditorData,
  AgentProfileMutationInput,
  AgentProfileSummary,
  AgentAvatarDraft,
  AgentAvatarHistoryAsset,
} from "../domain/agentManagement";
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
}
