export interface ContactItem {
  id: string;
  agentId: string;
  name: string;
  authorizedBuiltinMcpIds: string[];
  status: string;
  createdAt: Date;
  updatedAt: Date;
}
