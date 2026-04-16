export type AuthMode =
  | "password"
  | "tls_client_cert"
  | "token"
  | "integrated"
  | "file_key"
  | "no_auth";

export type DbType =
  | "postgres"
  | "mysql"
  | "sqlite"
  | "sql_server"
  | "oracle"
  | "mongodb";

export interface DbTypeDescriptor {
  db_type: DbType;
  label: string;
  auth_modes: AuthMode[];
  network_modes: string[];
  capabilities: {
    has_database_level: boolean;
    has_schema_level: boolean;
    supports_materialized_view: boolean;
    supports_synonym: boolean;
    supports_package: boolean;
    supports_trigger: boolean;
  };
}

export interface DbTypeListResponse {
  items: DbTypeDescriptor[];
}

export interface DataSourceListItem {
  id: string;
  name: string;
  db_type: DbType;
  status: "unknown" | "online" | "offline" | "degraded";
}

export interface DataSourceDetail extends DataSourceListItem {
  network: {
    mode: "direct" | "ssh_tunnel" | "proxy";
    host?: string | null;
    port?: number | null;
    database?: string | null;
    service_name?: string | null;
    sid?: string | null;
    file_path?: string | null;
    ssh: null;
  };
  auth: {
    mode: AuthMode;
    username?: string | null;
    password?: string | null;
    access_token?: string | null;
    client_cert?: string | null;
    client_key?: string | null;
    key_ref?: string | null;
    wallet_ref?: string | null;
    principal?: string | null;
    realm?: string | null;
    kdc?: string | null;
    service_name?: string | null;
  };
  tls?: {
    enabled: boolean;
    ssl_mode?: "disabled" | "preferred" | "required" | "verify_ca" | "verify_full" | null;
    ca_cert?: string | null;
    client_cert?: string | null;
    client_key?: string | null;
  } | null;
  options?: {
    connect_timeout_ms?: number | null;
    statement_timeout_ms?: number | null;
    pool_min?: number | null;
    pool_max?: number | null;
  } | null;
  tags?: string[];
}

export interface DataSourceListResponse {
  items: DataSourceListItem[];
  total: number;
}

export interface DataSourceMutationResponse {
  id: string;
  status: string;
}

export interface ConnectionTestResult {
  ok: boolean;
  latency_ms: number;
  server_version?: string;
  auth_mode: AuthMode;
  checks: Array<{ stage: string; ok: boolean; message?: string }>;
  error_code?: string;
  message?: string;
  stage?: string;
}

export interface DatabaseSummaryResponse {
  database_count: number;
  visible_database_count: number;
  visibility_scope: string;
}

export interface DatabaseInfo {
  name: string;
  owner?: string;
  size_bytes?: number;
}

export interface DatabaseListResponse {
  items: DatabaseInfo[];
  page: number;
  page_size: number;
  total: number;
}

export interface ObjectStatsResponse {
  database: string;
  schema_count?: number;
  table_count?: number;
  view_count?: number;
  materialized_view_count?: number;
  collection_count?: number;
  index_count?: number;
  procedure_count?: number;
  function_count?: number;
  trigger_count?: number;
  sequence_count?: number;
  synonym_count?: number;
  package_count?: number;
  partial: boolean;
}

export interface MetadataNode {
  id: string;
  parent_id: string;
  node_type: string;
  display_name: string;
  path: string;
  has_children: boolean;
}

export interface MetadataNodesResponse {
  items: MetadataNode[];
  page: number;
  page_size: number;
  total: number;
}

export interface ObjectColumn {
  name: string;
  data_type: string;
  nullable: boolean;
}

export interface ObjectIndex {
  name: string;
  columns: string[];
  is_unique: boolean;
}

export interface ObjectConstraint {
  name: string;
  constraint_type: string;
  columns: string[];
}

export interface ObjectDetailResponse {
  node_id: string;
  node_type: string;
  name: string;
  columns: ObjectColumn[];
  indexes: ObjectIndex[];
  constraints: ObjectConstraint[];
  ddl?: string;
}

export interface CreateDataSourcePayload {
  name: string;
  db_type: DbType;
  network: {
    mode: "direct" | "ssh_tunnel" | "proxy";
    host?: string | null;
    port?: number | null;
    database?: string | null;
    service_name?: string | null;
    sid?: string | null;
    file_path?: string | null;
    ssh: null;
  };
  auth: {
    mode: AuthMode;
    username?: string | null;
    password?: string | null;
    access_token?: string | null;
    client_cert?: string | null;
    client_key?: string | null;
    key_ref?: string | null;
    wallet_ref?: string | null;
    principal?: string | null;
    realm?: string | null;
    kdc?: string | null;
    service_name?: string | null;
  };
  tls?: {
    enabled: boolean;
    ssl_mode: "disabled" | "preferred" | "required" | "verify_ca" | "verify_full";
    ca_cert?: string | null;
    client_cert?: string | null;
    client_key?: string | null;
  } | null;
  options?: {
    connect_timeout_ms: number;
    statement_timeout_ms: number;
    pool_min: number;
    pool_max: number;
  };
  tags?: string[];
}

export interface UpdateDataSourcePayload {
  name?: string;
  network?: CreateDataSourcePayload["network"];
  auth?: CreateDataSourcePayload["auth"];
  tls?: CreateDataSourcePayload["tls"];
  options?: CreateDataSourcePayload["options"];
  tags?: string[];
}

export interface QueryColumn {
  name: string;
  type: string;
}

export interface QueryExecutePayload {
  datasource_id: string;
  database?: string | null;
  sql: string;
  timeout_ms?: number;
  max_rows?: number;
}

export interface QueryExecuteResponse {
  query_id: string;
  columns: QueryColumn[];
  rows: Array<Array<unknown>>;
  row_count: number;
  elapsed_ms: number;
}
