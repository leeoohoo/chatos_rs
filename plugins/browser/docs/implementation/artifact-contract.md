# Plugin MCP Artifact registration contract proposal

Browser MCP writes screenshots, HAR files, and collected downloads only below the
adapter-session-scoped `CHATOS_PLUGIN_ARTIFACT_DIR`. It never returns an absolute local path.

Every producer-side descriptor has this shape:

```json
{
  "artifact_id": "artifact_<opaque producer id>",
  "relative_path": "artifact_<opaque producer id>-screenshot.png",
  "display_name": "screenshot.png",
  "media_type": "image/png",
  "size_bytes": 12345,
  "sha256": "lowercase sha256"
}
```

The producer `artifact_id` is local to the Browser MCP process and is not a ChatOS registered
Artifact ID. Successful MCP tool results additionally publish bounded registration candidates in:

```json
{
  "_meta": {
    "chatos/artifacts": [
      {
        "producer_artifact_id": "artifact_<opaque producer id>",
        "relative_path": "artifact_<opaque producer id>-screenshot.png",
        "display_name": "screenshot.png",
        "media_type": "image/png",
        "size_bytes": 12345,
        "sha256": "lowercase sha256"
      }
    ]
  }
}
```

The proposed generic Host behavior is:

1. Resolve `relative_path` strictly below the current session's `CHATOS_PLUGIN_ARTIFACT_DIR`.
2. Reject symlinks, non-regular files, traversal, oversized files, and unsupported MIME types.
3. Recompute size and SHA-256 and compare them with the candidate.
4. Add owner identity from the immutable Plugin Runtime Session.
5. Register a platform Artifact and allocate the authoritative `pa_...` ID.
6. Return or project the registered descriptor to Task Runner and ChatOS UI.
7. Clean unregistered files when the adapter session closes.

The Host must never trust a producer-supplied absolute path or treat the producer artifact ID as a
platform Artifact ID.
