import type { AgentProfileSummary } from "../domain/agentManagement";

export interface AgentManagementPort {
  listProfiles(ownerUserId: string): Promise<AgentProfileSummary[]>;
}
