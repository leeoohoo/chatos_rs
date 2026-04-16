import type {
  DataSourceDetail,
  DbType,
  UpdateDataSourcePayload
} from "../../types/models";
import {
  buildPayload,
  type BuildPayloadInput
} from "../connections/connectionFormPayload";

export type MysqlScope = "all" | "single";

export interface ConnectionEditorValue extends Omit<BuildPayloadInput, "databaseScope"> {
  mysqlScope: MysqlScope;
}

export function defaultEditorValue(dbType: DbType = "postgres"): ConnectionEditorValue {
  const base: ConnectionEditorValue = {
    name: "",
    dbType,
    authMode: "password",
    host: "127.0.0.1",
    port: 5432,
    database: "postgres",
    serviceName: "",
    sid: "",
    filePath: "/tmp/db_connection_hub_demo.sqlite",
    username: "",
    password: "",
    accessToken: "",
    clientCert: "",
    clientKey: "",
    keyRef: "",
    mysqlScope: "all"
  };
  return applyDbTypeDefaults(base, dbType);
}

export function applyDbTypeDefaults(
  current: ConnectionEditorValue,
  dbType: DbType
): ConnectionEditorValue {
  const next = { ...current, dbType, mysqlScope: "all" as MysqlScope };
  if (dbType === "postgres") {
    next.port = 5432;
    next.database = "postgres";
    next.serviceName = "";
    next.sid = "";
  } else if (dbType === "mysql") {
    next.port = 3306;
    next.database = "";
    next.serviceName = "";
    next.sid = "";
  } else if (dbType === "sql_server") {
    next.port = 1433;
    next.database = "master";
    next.serviceName = "";
    next.sid = "";
  } else if (dbType === "oracle") {
    next.port = 1521;
    next.database = "";
    next.serviceName = "orclpdb1";
    next.sid = "";
  } else if (dbType === "mongodb") {
    next.port = 27017;
    next.database = "admin";
    next.serviceName = "";
    next.sid = "";
  }
  return next;
}

export function detailToEditorValue(detail: DataSourceDetail): ConnectionEditorValue {
  const keyRef = detail.auth.key_ref ?? detail.auth.wallet_ref ?? "";
  return {
    name: detail.name ?? "",
    dbType: detail.db_type,
    authMode: detail.auth.mode,
    host: detail.network.host ?? "127.0.0.1",
    port: detail.network.port ?? portByDbType(detail.db_type),
    database: detail.network.database ?? "",
    serviceName: detail.network.service_name ?? "",
    sid: detail.network.sid ?? "",
    filePath: detail.network.file_path ?? "/tmp/db_connection_hub_demo.sqlite",
    username: detail.auth.username ?? "",
    password: detail.auth.password ?? "",
    accessToken: detail.auth.access_token ?? "",
    clientCert: detail.auth.client_cert ?? "",
    clientKey: detail.auth.client_key ?? "",
    keyRef,
    mysqlScope:
      supportsAllScopeByDbType(detail.db_type) && (detail.network.database ?? "").trim() !== ""
        ? "single"
        : "all"
  };
}

export function toCreatePayload(value: ConnectionEditorValue) {
  const normalized = {
    ...value,
    database: supportsAllScopeByDbType(value.dbType) && value.mysqlScope === "all" ? "" : value.database,
    databaseScope: supportsAllScopeByDbType(value.dbType) ? value.mysqlScope : ("single" as const)
  };
  return buildPayload(normalized);
}

export function toUpdatePayload(value: ConnectionEditorValue): UpdateDataSourcePayload {
  const createPayload = toCreatePayload(value);
  return {
    name: createPayload.name,
    network: createPayload.network,
    auth: createPayload.auth,
    tls: createPayload.tls
  };
}

function portByDbType(dbType: DbType): number {
  if (dbType === "mysql") {
    return 3306;
  }
  if (dbType === "sql_server") {
    return 1433;
  }
  if (dbType === "oracle") {
    return 1521;
  }
  if (dbType === "mongodb") {
    return 27017;
  }
  return 5432;
}

export function supportsAllScopeByDbType(dbType: DbType): boolean {
  return dbType === "mysql" || dbType === "postgres" || dbType === "sql_server" || dbType === "mongodb";
}
