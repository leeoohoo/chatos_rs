import { useEffect, useMemo, useState } from "react";
import { apiClient } from "../../api/client";
import { validateBuildPayloadInput } from "../connections/connectionFormValidation";
import type { AuthMode, ConnectionTestResult, DbTypeDescriptor } from "../../types/models";
import {
  applyDbTypeDefaults,
  defaultEditorValue,
  supportsAllScopeByDbType,
  toCreatePayload,
  type ConnectionEditorValue
} from "./connectionFormAdapter";

interface Props {
  open: boolean;
  mode: "create" | "edit";
  dbTypes: DbTypeDescriptor[];
  initialValue?: ConnectionEditorValue | null;
  submitting: boolean;
  onClose: () => void;
  onSubmit: (value: ConnectionEditorValue) => Promise<void>;
}

export function ConnectionModal({
  open,
  mode,
  dbTypes,
  initialValue,
  submitting,
  onClose,
  onSubmit
}: Props) {
  const [value, setValue] = useState<ConnectionEditorValue>(defaultEditorValue("postgres"));
  const [error, setError] = useState<string | null>(null);
  const [mysqlDatabases, setMysqlDatabases] = useState<string[]>([]);
  const [loadingMysqlDatabases, setLoadingMysqlDatabases] = useState(false);
  const [mysqlDatabasesError, setMysqlDatabasesError] = useState<string | null>(null);
  const [testingConnection, setTestingConnection] = useState(false);
  const [testResult, setTestResult] = useState<ConnectionTestResult | null>(null);

  const selectedDbType = useMemo(
    () => dbTypes.find((item) => item.db_type === value.dbType) || null,
    [dbTypes, value.dbType]
  );
  const availableAuthModes = selectedDbType?.auth_modes || ["password"];
  const isSqlite = value.dbType === "sqlite";
  const supportsDatabaseScope = supportsAllScopeByDbType(value.dbType);
  const requireSqlServerCredentials =
    value.dbType === "sql_server" && value.authMode === "tls_client_cert";

  const authReady =
    (value.authMode === "password" &&
      value.username.trim() !== "" &&
      value.password.trim() !== "") ||
    (value.authMode === "token" && value.accessToken.trim() !== "") ||
    (value.authMode === "tls_client_cert" &&
      value.clientCert.trim() !== "" &&
      value.clientKey.trim() !== "") ||
    (value.authMode === "file_key" && value.keyRef.trim() !== "") ||
    value.authMode === "integrated" ||
    value.authMode === "no_auth";

  const canDiscoverDatabases =
    supportsDatabaseScope &&
    value.mysqlScope === "single" &&
    value.host.trim() !== "" &&
    Number.isInteger(value.port) &&
    value.port >= 1 &&
    value.port <= 65535 &&
    authReady;

  useEffect(() => {
    if (!open) {
      return;
    }
    if (mode === "edit" && initialValue) {
      setValue(initialValue);
    } else {
      const firstDbType = dbTypes[0]?.db_type || "postgres";
      setValue(defaultEditorValue(firstDbType));
    }
    setError(null);
    setMysqlDatabases([]);
    setMysqlDatabasesError(null);
    setTestResult(null);
  }, [open, mode, initialValue, dbTypes]);

  useEffect(() => {
    if (!availableAuthModes.includes(value.authMode)) {
      setValue((prev) => ({ ...prev, authMode: availableAuthModes[0] || "password" }));
    }
  }, [availableAuthModes, value.authMode]);

  useEffect(() => {
    if (!canDiscoverDatabases) {
      return;
    }
    void discoverMysqlDatabases();
  }, [
    canDiscoverDatabases,
    value.name,
    value.host,
    value.port,
    value.authMode,
    value.username,
    value.password,
    value.accessToken,
    value.clientCert,
    value.clientKey,
    value.keyRef
  ]);

  if (!open) {
    return null;
  }

  async function discoverMysqlDatabases() {
    try {
      setMysqlDatabasesError(null);
      setLoadingMysqlDatabases(true);
      const payload = toCreatePayload({
        ...value,
        mysqlScope: "all",
        database: ""
      });
      const response = await apiClient.discoverDatabases(payload, { page: 1, pageSize: 500 });
      const items = response.items.map((item) => item.name).filter((item) => item.trim() !== "");
      setMysqlDatabases(items);
      if (value.mysqlScope === "single" && value.database.trim() === "" && items.length > 0) {
        setValue((prev) => ({ ...prev, database: items[0] }));
      }
    } catch (caught) {
      setMysqlDatabases([]);
      setMysqlDatabasesError(caught instanceof Error ? caught.message : "Discover databases failed");
    } finally {
      setLoadingMysqlDatabases(false);
    }
  }

  function update<K extends keyof ConnectionEditorValue>(key: K, next: ConnectionEditorValue[K]) {
    setTestResult(null);
    setValue((prev) => ({ ...prev, [key]: next }));
  }

  function normalizeEditorValue(input: ConnectionEditorValue) {
    return {
      ...input,
      databaseScope: supportsAllScopeByDbType(input.dbType) ? input.mysqlScope : ("single" as const),
      database: supportsAllScopeByDbType(input.dbType) && input.mysqlScope === "all" ? "" : input.database
    };
  }

  function validateSingleScopeSelection(input: ConnectionEditorValue) {
    if (supportsAllScopeByDbType(input.dbType) && input.mysqlScope === "single") {
      if (input.database.trim() === "") {
        throw new Error("please choose one database");
      }
    }
  }

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);

    try {
      validateSingleScopeSelection(value);
      const normalized = normalizeEditorValue(value);
      validateBuildPayloadInput(normalized);
      await onSubmit(normalized);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Save connection failed");
    }
  }

  async function handleTestConnection() {
    setError(null);
    setTestResult(null);
    setTestingConnection(true);

    try {
      validateSingleScopeSelection(value);
      const normalized = normalizeEditorValue(value);
      validateBuildPayloadInput(normalized);
      const payload = toCreatePayload(normalized);
      const result = await apiClient.testConnectionPreview(payload);
      setTestResult(result);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Test connection failed");
    } finally {
      setTestingConnection(false);
    }
  }

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal-card" onClick={(event) => event.stopPropagation()}>
        <div className="modal-header">
          <h2>{mode === "create" ? "New Connection" : "Edit Connection"}</h2>
          <button type="button" className="ghost-btn" onClick={onClose} disabled={submitting}>
            Close
          </button>
        </div>

        <form className="modal-form-grid" onSubmit={handleSubmit}>
          <label>
            Name
            <input value={value.name} onChange={(event) => update("name", event.target.value)} required />
          </label>

          <label>
            Database Type
            <select
              value={value.dbType}
              disabled={mode === "edit"}
              onChange={(event) => {
                const dbType = event.target.value as ConnectionEditorValue["dbType"];
                update("dbType", dbType);
                setValue((prev) => applyDbTypeDefaults({ ...prev, dbType }, dbType));
                setMysqlDatabases([]);
                setMysqlDatabasesError(null);
              }}
            >
              {dbTypes.map((item) => (
                <option key={item.db_type} value={item.db_type}>
                  {item.label}
                </option>
              ))}
            </select>
          </label>

          <label>
            Auth Mode
            <select
              value={value.authMode}
              onChange={(event) => update("authMode", event.target.value as AuthMode)}
            >
              {availableAuthModes.map((item) => (
                <option key={item} value={item}>
                  {item}
                </option>
              ))}
            </select>
          </label>

          {!isSqlite ? (
            <>
              <label>
                Host
                <input value={value.host} onChange={(event) => update("host", event.target.value)} required />
              </label>

              <label>
                Port
                <input
                  type="number"
                  value={value.port}
                  onChange={(event) => update("port", Number(event.target.value))}
                  required
                />
              </label>

              {supportsDatabaseScope ? (
                <>
                  <label>
                    Database Scope
                    <select
                      value={value.mysqlScope}
                      onChange={(event) => {
                        const scope = event.target.value as ConnectionEditorValue["mysqlScope"];
                        update("mysqlScope", scope);
                        if (scope === "all") {
                          update("database", "");
                        }
                      }}
                    >
                      <option value="all">All Databases</option>
                      <option value="single">Single Database</option>
                    </select>
                  </label>

                  {value.mysqlScope === "single" ? (
                    <label>
                      Database
                      <select
                        value={value.database}
                        onChange={(event) => update("database", event.target.value)}
                        disabled={loadingMysqlDatabases}
                      >
                        <option value="">
                          {loadingMysqlDatabases ? "Loading..." : "Select database"}
                        </option>
                        {mysqlDatabases.map((item) => (
                          <option key={item} value={item}>
                            {item}
                          </option>
                        ))}
                      </select>
                    </label>
                  ) : null}

                  {value.mysqlScope === "single" ? (
                    <button
                      type="button"
                      className="ghost-btn"
                      onClick={() => void discoverMysqlDatabases()}
                      disabled={loadingMysqlDatabases}
                    >
                      {loadingMysqlDatabases ? "Refreshing..." : "Refresh Databases"}
                    </button>
                  ) : null}

                  {mysqlDatabasesError ? <p className="error">{mysqlDatabasesError}</p> : null}
                </>
              ) : (
                <label>
                  {value.dbType === "oracle" ? "Database / PDB (Optional)" : "Database"}
                  <input
                    value={value.database}
                    onChange={(event) => update("database", event.target.value)}
                    required={value.dbType !== "oracle"}
                  />
                </label>
              )}

              {value.dbType === "oracle" ? (
                <>
                  <label>
                    Service Name (Optional)
                    <input
                      value={value.serviceName}
                      onChange={(event) => update("serviceName", event.target.value)}
                    />
                  </label>
                  <label>
                    SID (Optional)
                    <input value={value.sid} onChange={(event) => update("sid", event.target.value)} />
                  </label>
                </>
              ) : null}
            </>
          ) : (
            <label>
              SQLite File Path
              <input
                value={value.filePath}
                onChange={(event) => update("filePath", event.target.value)}
                required
              />
            </label>
          )}

          {value.authMode === "password" ? (
            <>
              <label>
                Username
                <input
                  value={value.username}
                  onChange={(event) => update("username", event.target.value)}
                  required
                />
              </label>
              <label>
                Password
                <input
                  type="password"
                  value={value.password}
                  onChange={(event) => update("password", event.target.value)}
                  required
                />
              </label>
            </>
          ) : null}

          {value.authMode === "token" ? (
            <>
              <label>
                Username (Optional)
                <input value={value.username} onChange={(event) => update("username", event.target.value)} />
              </label>
              <label>
                Access Token
                <input
                  type="password"
                  value={value.accessToken}
                  onChange={(event) => update("accessToken", event.target.value)}
                  required
                />
              </label>
            </>
          ) : null}

          {value.authMode === "tls_client_cert" ? (
            <>
              <label>
                Client Cert
                <textarea
                  value={value.clientCert}
                  onChange={(event) => update("clientCert", event.target.value)}
                  required
                />
              </label>
              <label>
                Client Key
                <textarea
                  value={value.clientKey}
                  onChange={(event) => update("clientKey", event.target.value)}
                  required
                />
              </label>

              {requireSqlServerCredentials ? (
                <>
                  <label>
                    Username
                    <input
                      value={value.username}
                      onChange={(event) => update("username", event.target.value)}
                      required
                    />
                  </label>
                  <label>
                    Password
                    <input
                      type="password"
                      value={value.password}
                      onChange={(event) => update("password", event.target.value)}
                      required
                    />
                  </label>
                </>
              ) : null}
            </>
          ) : null}

          {value.authMode === "file_key" ? (
            <label>
              Key Ref / Wallet Ref
              <input value={value.keyRef} onChange={(event) => update("keyRef", event.target.value)} required />
            </label>
          ) : null}

          <div className="modal-actions">
            <button type="button" className="ghost-btn" onClick={onClose} disabled={submitting}>
              Cancel
            </button>
            <button
              type="button"
              className="ghost-btn"
              onClick={() => void handleTestConnection()}
              disabled={submitting || testingConnection}
            >
              {testingConnection ? "Testing..." : "Test Connection"}
            </button>
            <button type="submit" disabled={submitting}>
              {submitting ? "Saving..." : mode === "create" ? "Create" : "Save"}
            </button>
          </div>
        </form>

        {testResult ? (
          <p className={testResult.ok ? "success" : "error"}>
            {formatTestResult(testResult)}
          </p>
        ) : null}
        {error ? <p className="error">{error}</p> : null}
      </div>
    </div>
  );
}

function formatTestResult(result: ConnectionTestResult): string {
  if (result.ok) {
    const details: string[] = ["Connection OK"];
    if (result.server_version) {
      details.push(`version: ${result.server_version}`);
    }
    if (result.latency_ms > 0) {
      details.push(`latency: ${result.latency_ms} ms`);
    }
    return details.join(" · ");
  }

  return result.message ?? result.error_code ?? "Connection failed";
}
