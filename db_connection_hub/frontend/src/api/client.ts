import type {
  ConnectionTestResult,
  CreateDataSourcePayload,
  DataSourceDetail,
  DatabaseListResponse,
  DataSourceListResponse,
  DataSourceMutationResponse,
  DatabaseSummaryResponse,
  DbTypeListResponse,
  MetadataNodesResponse,
  ObjectStatsResponse,
  ObjectDetailResponse,
  QueryExecutePayload,
  QueryExecuteResponse,
  UpdateDataSourcePayload
} from "../types/models";

const API_BASE =
  (import.meta.env.VITE_API_BASE_URL as string | undefined)?.trim() ||
  "/api/v1";

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let response: Response;
  try {
    response = await fetch(`${API_BASE}${path}`, {
      ...init,
      headers: {
        "Content-Type": "application/json",
        ...(init?.headers || {})
      }
    });
  } catch (caught) {
    const displayBase = resolveApiBaseForDisplay();
    const detail = caught instanceof Error ? caught.message : "request was blocked";
    throw new Error(`Network error: cannot reach API (${displayBase}). ${detail}`);
  }

  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `HTTP ${response.status}`);
  }

  return (await response.json()) as T;
}

export const apiClient = {
  getDbTypes(): Promise<DbTypeListResponse> {
    return request<DbTypeListResponse>("/meta/db-types");
  },
  listDatasources(): Promise<DataSourceListResponse> {
    return request<DataSourceListResponse>("/datasources");
  },
  getDatasource(id: string): Promise<DataSourceDetail> {
    return request<DataSourceDetail>(`/datasources/${id}`);
  },
  createDatasource(payload: CreateDataSourcePayload): Promise<DataSourceMutationResponse> {
    return request<DataSourceMutationResponse>("/datasources", {
      method: "POST",
      body: JSON.stringify(payload)
    });
  },
  updateDatasource(
    id: string,
    payload: UpdateDataSourcePayload
  ): Promise<DataSourceMutationResponse> {
    return request<DataSourceMutationResponse>(`/datasources/${id}`, {
      method: "PUT",
      body: JSON.stringify(payload)
    });
  },
  deleteDatasource(id: string): Promise<DataSourceMutationResponse> {
    return request<DataSourceMutationResponse>(`/datasources/${id}`, {
      method: "DELETE"
    });
  },
  testDatasource(id: string): Promise<ConnectionTestResult> {
    return request<ConnectionTestResult>(`/datasources/${id}/test`, {
      method: "POST"
    });
  },
  getDatabaseSummary(id: string): Promise<DatabaseSummaryResponse> {
    return request<DatabaseSummaryResponse>(`/datasources/${id}/databases/summary`);
  },
  listDatabases(
    id: string,
    options?: { keyword?: string; page?: number; pageSize?: number }
  ): Promise<DatabaseListResponse> {
    const query = new URLSearchParams({
      page: String(options?.page ?? 1),
      page_size: String(options?.pageSize ?? 200)
    });
    if (options?.keyword) {
      query.set("keyword", options.keyword);
    }
    return request<DatabaseListResponse>(`/datasources/${id}/databases?${query.toString()}`);
  },
  getObjectStats(id: string, database: string): Promise<ObjectStatsResponse> {
    return request<ObjectStatsResponse>(
      `/datasources/${id}/databases/${encodeURIComponent(database)}/object-stats`
    );
  },
  getNodes(
    datasourceId: string,
    options?: { parentId?: string; page?: number; pageSize?: number }
  ): Promise<MetadataNodesResponse> {
    const page = options?.page ?? 1;
    const pageSize = options?.pageSize ?? 100;

    const query = new URLSearchParams({
      datasource_id: datasourceId,
      page: String(page),
      page_size: String(pageSize)
    });
    if (options?.parentId) {
      query.set("parent_id", options.parentId);
    }
    return request<MetadataNodesResponse>(`/metadata/nodes?${query.toString()}`);
  },
  getObjectDetail(datasourceId: string, nodeId: string): Promise<ObjectDetailResponse> {
    const query = new URLSearchParams({
      datasource_id: datasourceId,
      node_id: nodeId
    });
    return request<ObjectDetailResponse>(`/metadata/object-detail?${query.toString()}`);
  },
  discoverDatabases(
    payload: CreateDataSourcePayload,
    options?: { keyword?: string; page?: number; pageSize?: number }
  ): Promise<DatabaseListResponse> {
    const query = new URLSearchParams({
      page: String(options?.page ?? 1),
      page_size: String(options?.pageSize ?? 200)
    });
    if (options?.keyword) {
      query.set("keyword", options.keyword);
    }

    return request<DatabaseListResponse>(`/meta/discover-databases?${query.toString()}`, {
      method: "POST",
      body: JSON.stringify(payload)
    });
  },
  testConnectionPreview(payload: CreateDataSourcePayload): Promise<ConnectionTestResult> {
    return request<ConnectionTestResult>("/meta/test-connection", {
      method: "POST",
      body: JSON.stringify(payload)
    });
  },
  executeQuery(payload: QueryExecutePayload): Promise<QueryExecuteResponse> {
    return request<QueryExecuteResponse>("/queries/execute", {
      method: "POST",
      body: JSON.stringify(payload)
    });
  }
};

function resolveApiBaseForDisplay(): string {
  try {
    return new URL(API_BASE, window.location.origin).toString();
  } catch {
    return API_BASE;
  }
}
