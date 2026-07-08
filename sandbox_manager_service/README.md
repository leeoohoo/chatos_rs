# Sandbox Manager Service

Sandbox Manager manages sandbox leases and proxies MCP calls to sandbox agents. In the Docker cloud stack it runs as a container while controlling the host Docker daemon through `/var/run/docker.sock`.

## Run

From the repository root:

```bash
docker/deploy.sh up
```

Default URLs:

- Frontend: `http://localhost:8096`
- Backend: `http://localhost:8095`

Health check:

```bash
curl http://127.0.0.1:8095/health
```

## Docker Backend

Compose sets the backend to Docker mode:

```env
SANDBOX_MANAGER_BACKEND=docker
SANDBOX_MANAGER_DOCKER_NETWORK=chatos-cloud
SANDBOX_MANAGER_DOCKER_AGENT_ENDPOINT_MODE=container
SANDBOX_MANAGER_DOCKER_PUBLISH_AGENT=false
```

The manager creates containers from `chatos-sandbox-agent:latest` and attaches them to the same Docker bridge network as the Compose services. Agent URLs are container-local, for example:

```text
http://chatos-sandbox-<sandbox_id>:49888
```

This avoids publishing every agent port on the host.

Important: mounting `/var/run/docker.sock` gives the manager high privilege over the host Docker daemon.

## Auth

`/health` is public. Other API routes can require auth with:

```env
SANDBOX_MANAGER_REQUIRE_AUTH=true
```

Supported callers:

- system client: `x-sandbox-client-id` + `x-sandbox-client-key`
- operator: `x-sandbox-operator-token`
- user token: `Authorization: Bearer ...`, verified through `user_service`

Task Runner uses the system client credentials from `docker/.env`.

## API Examples

Create a lease:

```bash
curl -X POST http://127.0.0.1:8095/api/sandboxes/leases \
  -H 'content-type: application/json' \
  -d '{
    "tenant_id": "tenant-dev",
    "user_id": "user-dev",
    "project_id": "project-dev",
    "run_id": "run-dev-1",
    "workspace_root": "/workspace",
    "tools": ["filesystem", "terminal"],
    "ttl_seconds": 3600
  }'
```

List sandboxes:

```bash
curl http://127.0.0.1:8095/api/sandboxes
```

Pool status:

```bash
curl http://127.0.0.1:8095/api/sandbox-pool/status
```
