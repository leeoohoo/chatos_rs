// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface PagingOptions {
  limit?: number;
  offset?: number;
}

export interface SessionPagingOptions extends PagingOptions {
  includeArchived?: boolean;
  includeArchiving?: boolean;
}

export interface MemoryAgentsQueryOptions extends PagingOptions {
  enabled?: boolean;
}

export interface MemorySkillsQueryOptions extends PagingOptions {
  plugin_source?: string;
  query?: string;
}

export interface MemoryAgentSessionsQueryOptions extends PagingOptions {
  project_id?: string;
  status?: string;
}

export interface DeleteSuccessResponse {
  success?: boolean;
  deleted?: boolean;
  message?: string;
}
