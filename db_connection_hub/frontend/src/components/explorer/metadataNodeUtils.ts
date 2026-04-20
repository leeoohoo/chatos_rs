import type { DbType } from "../../types/models";

export function parseDatabaseFromNodeId(nodeId: string | null | undefined): string | null {
  if (!nodeId || nodeId === "root") {
    return null;
  }

  if (nodeId.startsWith("db:")) {
    return nodeId.slice("db:".length) || null;
  }

  const parts = nodeId.split(":");
  if (parts.length < 2) {
    return null;
  }
  return parts[1] || null;
}

export function supportsDetailByNodeId(nodeId: string): boolean {
  const common =
    nodeId.startsWith("table:") ||
    nodeId.startsWith("collection:") ||
    nodeId.startsWith("view:") ||
    nodeId.startsWith("materialized_view:") ||
    nodeId.startsWith("sequence:") ||
    nodeId.startsWith("synonym:") ||
    nodeId.startsWith("package:");

  return common;
}

export function supportsDetailByNodeIdForDb(nodeId: string, dbType: DbType | null | undefined): boolean {
  if (supportsDetailByNodeId(nodeId)) {
    return true;
  }

  if (dbType === "oracle") {
    return (
      nodeId.startsWith("procedure:") ||
      nodeId.startsWith("function:") ||
      nodeId.startsWith("index:") ||
      nodeId.startsWith("trigger:")
    );
  }

  if (dbType === "postgres" || dbType === "mysql" || dbType === "sqlite") {
    return nodeId.startsWith("index:") || nodeId.startsWith("trigger:");
  }

  if (dbType === "sql_server") {
    return (
      nodeId.startsWith("index:") ||
      nodeId.startsWith("trigger:") ||
      nodeId.startsWith("procedure:") ||
      nodeId.startsWith("function:")
    );
  }

  if (dbType === "mongodb") {
    return nodeId.startsWith("index:");
  }

  return false;
}
