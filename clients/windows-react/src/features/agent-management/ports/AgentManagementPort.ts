import type {
  AgentProfileEditorCatalog,
  AgentProfileEditorData,
  AgentProfileMutationInput,
  AgentProfileSummary,
} from "../domain/agentManagement";

export interface AgentManagementPort {
  listProfiles(ownerUserId: string): Promise<AgentProfileSummary[]>;
  loadEditorCatalog(ownerUserId: string): Promise<AgentProfileEditorCatalog>;
  loadProfileEditor(ownerUserId: string, agentId: string): Promise<AgentProfileEditorData>;
  saveProfile(ownerUserId: string, input: AgentProfileMutationInput): Promise<AgentProfileSummary>;
}
