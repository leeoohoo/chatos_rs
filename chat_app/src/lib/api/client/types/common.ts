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

export interface DeleteSuccessResponse {
  success?: boolean;
  deleted?: boolean;
  message?: string;
}
