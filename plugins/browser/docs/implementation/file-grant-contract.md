# Local Connector file grant contract

`browser_upload` never accepts a filesystem path. It accepts one or more opaque `file_grant_id`
values created by Local Connector after the user selects files.

Until the host-side file-grant API is finalized, the MCP uses this provisional local contract:

1. Local Connector creates an adapter-session-private directory and exposes it as
   `CHATOS_PLUGIN_FILE_GRANT_DIR`.
2. A grant named `grant_abc` is represented by `grant_abc.json` inside that directory.
3. The descriptor has the following shape:

```json
{
  "path": "/host-selected-file.txt",
  "expires_at_unix_ms": 1787241600000,
  "size": 1234,
  "sha256": "lowercase-or-uppercase-sha256"
}
```

The MCP enforces:

- grant IDs contain only ASCII letters, digits, `_`, and `-` and are at most 128 characters;
- descriptor files are resolved only below the injected grant directory and are at most 64 KiB;
- grants must be unexpired;
- selected files must be regular files no larger than 128 MiB;
- current file size and SHA-256 must match the signed/trusted descriptor values;
- a grant can be consumed only once per browser session;
- resolved paths are used only internally for `DOM.setFileInputFiles` and are never returned.

Local Connector must isolate this directory by adapter session, create descriptors only after an
explicit user selection, protect it from untrusted writes, and remove grants when the adapter
session closes. A future native grant RPC can replace this directory transport without changing
the `browser_upload(file_grant_ids)` MCP schema.
