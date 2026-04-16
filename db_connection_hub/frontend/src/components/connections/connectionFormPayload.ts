import type { AuthMode, CreateDataSourcePayload } from "../../types/models";

export interface BuildPayloadInput {
  name: string;
  dbType: CreateDataSourcePayload["db_type"];
  databaseScope: "all" | "single";
  authMode: AuthMode;
  host: string;
  port: number;
  database: string;
  serviceName: string;
  sid: string;
  filePath: string;
  username: string;
  password: string;
  accessToken: string;
  clientCert: string;
  clientKey: string;
  keyRef: string;
}

export function buildPayload(input: BuildPayloadInput): CreateDataSourcePayload {
  const commonAuth: CreateDataSourcePayload["auth"] = {
    mode: input.authMode,
    username: null,
    password: null,
    access_token: null,
    client_cert: null,
    client_key: null,
    key_ref: null,
    wallet_ref: null,
    principal: null,
    realm: null,
    kdc: null,
    service_name: null
  };

  if (input.authMode === "password") {
    commonAuth.username = input.username;
    commonAuth.password = input.password;
  } else if (input.authMode === "token") {
    commonAuth.username = input.username || null;
    commonAuth.access_token = input.accessToken;
  } else if (input.authMode === "tls_client_cert") {
    commonAuth.client_cert = input.clientCert;
    commonAuth.client_key = input.clientKey;
  } else if (input.authMode === "file_key") {
    commonAuth.key_ref = input.keyRef;
  }

  if (input.dbType === "sqlite") {
    return {
      name: input.name,
      db_type: input.dbType,
      network: {
        mode: "direct",
        host: null,
        port: null,
        database: null,
        service_name: null,
        sid: null,
        file_path: input.filePath,
        ssh: null
      },
      auth: commonAuth,
      tls: null,
      options: {
        connect_timeout_ms: 3000,
        statement_timeout_ms: 10000,
        pool_min: 1,
        pool_max: 3
      },
      tags: ["sqlite", "local"]
    };
  }

  return {
    name: input.name,
    db_type: input.dbType,
    network: {
      mode: "direct",
      host: input.host,
      port: input.port,
      database: input.databaseScope === "all" ? null : toOptional(input.database),
      service_name: toOptional(input.serviceName),
      sid: toOptional(input.sid),
      file_path: null,
      ssh: null
    },
    auth: commonAuth,
    tls: {
      enabled: false,
      ssl_mode: "disabled",
      ca_cert: null,
      client_cert: null,
      client_key: null
    },
    options: {
      connect_timeout_ms: 5000,
      statement_timeout_ms: 15000,
      pool_min: 1,
      pool_max: 20
    },
    tags: ["default"]
  };
}

function toOptional(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}
