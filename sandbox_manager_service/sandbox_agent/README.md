# ChatOS Sandbox Agent Image

This image is the first Docker/Kata sandbox runtime for `sandbox_manager_service`.

Build:

```bash
docker build -t chatos-sandbox-agent:latest -f sandbox_manager_service/sandbox_agent/Dockerfile .
```

Run locally:

```bash
docker run --rm -p 127.0.0.1:49888:49888 \
  -v /tmp/chatos-sandbox-demo:/workspace:rw \
  chatos-sandbox-agent:latest
```

Health check:

```bash
curl http://127.0.0.1:49888/health
```

Endpoints:

```text
GET  /health
POST /mcp        # JSON-RPC: tools/list, tools/call
GET  /mcp/tools  # compatibility helper
POST /mcp/call   # compatibility helper
POST /terminal/exec
POST /files/read
POST /files/write
POST /files/list
POST /files/mkdir
```

The JSON-RPC endpoint exposes the reused built-in file/code maintainer tools and terminal controller tools. All file paths and terminal working directories are resolved under `/workspace`.
