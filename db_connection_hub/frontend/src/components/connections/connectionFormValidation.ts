import type { BuildPayloadInput } from "./connectionFormPayload";

export function validateBuildPayloadInput(input: BuildPayloadInput): void {
  requireNonBlank("connection name", input.name);

  if (input.dbType === "sqlite") {
    requireNonBlank("sqlite file path", input.filePath);
  } else {
    requireNonBlank("host", input.host);
    if (input.dbType === "oracle") {
      const hasOracleTarget =
        input.database.trim() !== "" ||
        input.serviceName.trim() !== "" ||
        input.sid.trim() !== "";
      if (!hasOracleTarget) {
        throw new Error("oracle requires database or service name or sid");
      }
    } else if (
      input.databaseScope === "all" &&
      (input.dbType === "mysql" ||
        input.dbType === "postgres" ||
        input.dbType === "sql_server" ||
        input.dbType === "mongodb")
    ) {
      // database-level engines can use all-database scope by leaving network.database empty
    } else {
      requireNonBlank("database", input.database);
    }

    if (!Number.isInteger(input.port) || input.port < 1 || input.port > 65535) {
      throw new Error("port must be an integer between 1 and 65535");
    }
  }

  switch (input.authMode) {
    case "password":
      requireNonBlank("username", input.username);
      requireNonBlank("password", input.password);
      break;
    case "token":
      requireNonBlank("access token", input.accessToken);
      break;
    case "tls_client_cert":
      requireNonBlank("client cert", input.clientCert);
      requireNonBlank("client key", input.clientKey);
      // Current SQL Server real driver still requires username/password.
      if (input.dbType === "sql_server") {
        requireNonBlank("username", input.username);
        requireNonBlank("password", input.password);
      }
      break;
    case "file_key":
      requireNonBlank("key ref", input.keyRef);
      break;
    case "integrated":
    case "no_auth":
      break;
  }
}

function requireNonBlank(label: string, value: string): void {
  if (value.trim() === "") {
    throw new Error(`${label} is required`);
  }
}
