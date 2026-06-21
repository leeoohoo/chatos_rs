export interface ContactItem {
  id: string;
  agentId: string;
  name: string;
  status: string;
  taskRunner?: {
    enabled: boolean;
    baseUrl: string;
    agentAccountId?: string | null;
    username: string;
    hasPassword: boolean;
  };
  createdAt: Date;
  updatedAt: Date;
}
