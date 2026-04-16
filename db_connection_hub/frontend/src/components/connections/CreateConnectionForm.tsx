import { useEffect, useMemo, useState } from "react";
import { apiClient } from "../../api/client";
import type { AuthMode, CreateDataSourcePayload, DbTypeDescriptor } from "../../types/models";
import { buildPayload } from "./connectionFormPayload";
import { validateBuildPayloadInput } from "./connectionFormValidation";

interface Props {
  dbTypes: DbTypeDescriptor[];
  loading: boolean;
  onCreate: (payload: CreateDataSourcePayload) => Promise<void>;
}

export function CreateConnectionForm({ dbTypes, loading, onCreate }: Props) {
  const firstDbType = useMemo(() => dbTypes[0]?.db_type || "postgres", [dbTypes]);

  const [name, setName] = useState("orders-prod");
  const [dbType, setDbType] = useState(firstDbType);
  const [authMode, setAuthMode] = useState<AuthMode>("password");
  const [mysqlScope, setMysqlScope] = useState<"all" | "single">("all");

  const [host, setHost] = useState("127.0.0.1");
  const [port, setPort] = useState(5432);
  const [database, setDatabase] = useState("postgres");
  const [serviceName, setServiceName] = useState("");
  const [sid, setSid] = useState("");
  const [filePath, setFilePath] = useState("/tmp/db_connection_hub_demo.sqlite");

  const [username, setUsername] = useState("readonly");
  const [password, setPassword] = useState("secret");
  const [accessToken, setAccessToken] = useState("");
  const [clientCert, setClientCert] = useState("");
  const [clientKey, setClientKey] = useState("");
  const [keyRef, setKeyRef] = useState("");

  const [mysqlDatabases, setMysqlDatabases] = useState<string[]>([]);
  const [loadingMysqlDatabases, setLoadingMysqlDatabases] = useState(false);
  const [mysqlDatabasesError, setMysqlDatabasesError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const selectedDb = useMemo(
    () => dbTypes.find((item) => item.db_type === dbType) || null,
    [dbType, dbTypes]
  );

  const availableAuthModes = selectedDb?.auth_modes || ["password"];
  const isSqlite = dbType === "sqlite";
  const isMysql = dbType === "mysql";
  const requireSqlServerCredentials = dbType === "sql_server" && authMode === "tls_client_cert";
  const shouldDiscoverMysqlDatabases =
    isMysql &&
    mysqlScope === "single" &&
    host.trim() !== "" &&
    Number.isInteger(port) &&
    port >= 1 &&
    port <= 65535 &&
    ((authMode === "password" && username.trim() !== "" && password.trim() !== "") ||
      (authMode === "token" && accessToken.trim() !== "") ||
      (authMode === "tls_client_cert" && clientCert.trim() !== "" && clientKey.trim() !== ""));

  useEffect(() => {
    if (!availableAuthModes.includes(authMode)) {
      setAuthMode(availableAuthModes[0] || "password");
    }
  }, [authMode, availableAuthModes]);

  useEffect(() => {
    if (dbType === "postgres") {
      setPort(5432);
      setDatabase("postgres");
      setServiceName("");
      setSid("");
      setMysqlScope("all");
      setMysqlDatabases([]);
      setMysqlDatabasesError(null);
    } else if (dbType === "mysql") {
      setPort(3306);
      setDatabase("");
      setServiceName("");
      setSid("");
      setMysqlScope("all");
      setMysqlDatabases([]);
      setMysqlDatabasesError(null);
    } else if (dbType === "sql_server") {
      setPort(1433);
      setDatabase("master");
      setServiceName("");
      setSid("");
      setMysqlScope("all");
      setMysqlDatabases([]);
      setMysqlDatabasesError(null);
    } else if (dbType === "oracle") {
      setPort(1521);
      setDatabase("");
      setServiceName("orclpdb1");
      setSid("");
      setMysqlScope("all");
      setMysqlDatabases([]);
      setMysqlDatabasesError(null);
    } else if (dbType === "mongodb") {
      setPort(27017);
      setDatabase("admin");
      setServiceName("");
      setSid("");
      setMysqlScope("all");
      setMysqlDatabases([]);
      setMysqlDatabasesError(null);
    }
  }, [dbType]);

  useEffect(() => {
    if (!shouldDiscoverMysqlDatabases) {
      return;
    }
    void discoverMysqlDatabases();
  }, [
    shouldDiscoverMysqlDatabases,
    name,
    authMode,
    host,
    port,
    username,
    password,
    accessToken,
    clientCert,
    clientKey,
    keyRef
  ]);

  async function discoverMysqlDatabases() {
    try {
      setMysqlDatabasesError(null);
      setLoadingMysqlDatabases(true);

      const payload = buildPayload({
        name,
        dbType,
        databaseScope: dbType === "mysql" ? mysqlScope : "single",
        authMode,
        host,
        port,
        database: "",
        serviceName,
        sid,
        filePath,
        username,
        password,
        accessToken,
        clientCert,
        clientKey,
        keyRef
      });

      const response = await apiClient.discoverDatabases(payload, {
        page: 1,
        pageSize: 500
      });
      const items = response.items.map((item) => item.name).filter((item) => item.trim() !== "");
      setMysqlDatabases(items);

      if (mysqlScope === "single" && database.trim() === "" && items.length > 0) {
        setDatabase(items[0]);
      }
    } catch (caught) {
      setMysqlDatabases([]);
      setMysqlDatabasesError(caught instanceof Error ? caught.message : "Discover databases failed");
    } finally {
      setLoadingMysqlDatabases(false);
    }
  }

  async function handleSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError(null);

    try {
      if (dbType === "mysql" && mysqlScope === "single" && database.trim() === "") {
        throw new Error("please choose one mysql database");
      }

      const input = {
        name,
        dbType,
        databaseScope: dbType === "mysql" ? mysqlScope : "single" as const,
        authMode,
        host,
        port,
        database,
        serviceName,
        sid,
        filePath,
        username,
        password,
        accessToken,
        clientCert,
        clientKey,
        keyRef
      };

      validateBuildPayloadInput(input);
      const payload = buildPayload(input);

      await onCreate(payload);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Create datasource failed");
    }
  }

  return (
    <section className="card">
      <h2>Create Connection</h2>
      <form className="form-grid" onSubmit={handleSubmit}>
        <label>
          Name
          <input value={name} onChange={(event) => setName(event.target.value)} required />
        </label>

        <label>
          Database Type
          <select value={dbType} onChange={(event) => setDbType(event.target.value as typeof dbType)}>
            {dbTypes.map((item) => (
              <option key={item.db_type} value={item.db_type}>
                {item.label}
              </option>
            ))}
          </select>
        </label>

        <label>
          Auth Mode
          <select value={authMode} onChange={(event) => setAuthMode(event.target.value as AuthMode)}>
            {availableAuthModes.map((mode) => (
              <option key={mode} value={mode}>
                {mode}
              </option>
            ))}
          </select>
        </label>

        {!isSqlite ? (
          <>
            <label>
              Host
              <input value={host} onChange={(event) => setHost(event.target.value)} required />
            </label>

            <label>
              Port
              <input
                type="number"
                value={port}
                onChange={(event) => setPort(Number(event.target.value))}
                required
              />
            </label>

            {dbType === "mysql" ? (
              <>
                <label>
                  Database Scope
                  <select
                    value={mysqlScope}
                    onChange={(event) => {
                      const scope = event.target.value as "all" | "single";
                      setMysqlScope(scope);
                      if (scope === "all") {
                        setDatabase("");
                        setMysqlDatabasesError(null);
                      }
                    }}
                  >
                    <option value="all">All Databases</option>
                    <option value="single">Single Database</option>
                  </select>
                </label>
                {mysqlScope === "single" ? (
                  <label>
                    Database
                    <select
                      value={database}
                      onChange={(event) => setDatabase(event.target.value)}
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
                {mysqlScope === "single" ? (
                  <button
                    type="button"
                    className="back-btn"
                    onClick={() => void discoverMysqlDatabases()}
                    disabled={loadingMysqlDatabases}
                  >
                    {loadingMysqlDatabases ? "Refreshing..." : "Refresh Databases"}
                  </button>
                ) : null}
                {mysqlScope === "single" ? (
                  <p className="hint">
                    {loadingMysqlDatabases
                      ? "Discovering databases..."
                      : `Found ${mysqlDatabases.length} database${mysqlDatabases.length === 1 ? "" : "s"}`}
                  </p>
                ) : null}
                {mysqlDatabasesError ? <p className="error">{mysqlDatabasesError}</p> : null}
              </>
            ) : (
              <label>
                {dbType === "oracle" ? "Database / PDB (Optional)" : "Database"}
                <input
                  value={database}
                  onChange={(event) => setDatabase(event.target.value)}
                  required={dbType !== "oracle"}
                />
              </label>
            )}

            {dbType === "oracle" ? (
              <>
                <label>
                  Service Name (Optional)
                  <input value={serviceName} onChange={(event) => setServiceName(event.target.value)} />
                </label>
                <label>
                  SID (Optional)
                  <input value={sid} onChange={(event) => setSid(event.target.value)} />
                </label>
              </>
            ) : null}
          </>
        ) : (
          <label>
            SQLite File Path
            <input value={filePath} onChange={(event) => setFilePath(event.target.value)} required />
          </label>
        )}

        {authMode === "password" ? (
          <>
            <label>
              Username
              <input value={username} onChange={(event) => setUsername(event.target.value)} required />
            </label>
            <label>
              Password
              <input
                type="password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                required
              />
            </label>
          </>
        ) : null}

        {authMode === "token" ? (
          <>
            <label>
              Username (Optional)
              <input value={username} onChange={(event) => setUsername(event.target.value)} />
            </label>
            <label>
              Access Token
              <input
                type="password"
                value={accessToken}
                onChange={(event) => setAccessToken(event.target.value)}
                required
              />
            </label>
          </>
        ) : null}

        {authMode === "tls_client_cert" ? (
          <>
            <label>
              Client Cert
              <textarea value={clientCert} onChange={(event) => setClientCert(event.target.value)} required />
            </label>
            <label>
              Client Key
              <textarea value={clientKey} onChange={(event) => setClientKey(event.target.value)} required />
            </label>

            {requireSqlServerCredentials ? (
              <>
                <label>
                  Username
                  <input value={username} onChange={(event) => setUsername(event.target.value)} required />
                </label>
                <label>
                  Password
                  <input
                    type="password"
                    value={password}
                    onChange={(event) => setPassword(event.target.value)}
                    required
                  />
                </label>
              </>
            ) : null}
          </>
        ) : null}

        {authMode === "file_key" ? (
          <label>
            Key Ref / Wallet Ref
            <input value={keyRef} onChange={(event) => setKeyRef(event.target.value)} required />
          </label>
        ) : null}

        <button type="submit" disabled={loading}>
          {loading ? "Creating..." : "Create"}
        </button>
      </form>

      {selectedDb ? (
        <p className="hint">Capabilities: {Object.entries(selectedDb.capabilities).filter(([, v]) => v).map(([k]) => k).join(", ")}</p>
      ) : null}

      {dbType === "oracle" ? (
        <p className="hint">Oracle requires one target identifier: database, service name, or sid.</p>
      ) : null}

      {dbType === "mysql" ? (
        <p className="hint">MySQL supports single database or all databases scope.</p>
      ) : null}

      {error ? <p className="error">{error}</p> : null}
    </section>
  );
}
