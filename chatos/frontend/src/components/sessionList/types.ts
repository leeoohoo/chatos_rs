// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
