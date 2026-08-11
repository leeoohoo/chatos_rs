# Sandbox Manager Service

Sandbox Manager manages sandbox leases and proxies MCP calls to sandbox agents. In the Docker cloud stack it runs as a container and reaches the host Docker daemon through a restricted Docker Socket Proxy.

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

Compose does not mount `/var/run/docker.sock` into Sandbox Manager. Docker CLI requests are sent to
the private `sandbox-docker-socket-proxy` service, which exposes only the container, image, build,
BuildKit session/exec, volume, network, info, ping and version API groups required by the manager.
BuildKit uses its own managed container and state volume, so those narrowly scoped groups are
required for cloud image builds and cache GC. The proxy still permits container and image lifecycle
operations, so it must remain private and must never publish port `2375` on the host.

On Docker Desktop for macOS, image builds may require the Desktop-managed proxy even when the host
shell does not export `HTTP_PROXY` or `HTTPS_PROXY`. `sandbox_manager_service_backend` now
auto-detects the Docker engine proxy values from `docker info` only for image build arguments, so
service-to-service calls such as Configuration Center mTLS do not inherit those proxy settings. If
manual builds still time out during `apt-get update`, confirm that `docker info` shows reachable
`HTTP Proxy` / `HTTPS Proxy` values and prefer those for ad-hoc `docker build` commands as well.

Sandbox Manager labels every dynamically built image with its owning environment and service. It
removes a replaced image only after the replacement container has started, and deletes environment
images by their ownership labels when the environment is destroyed. BuildKit garbage collection is
enabled by default and caps the shared builder cache at 32 GB while reserving 8 GB:

```env
SANDBOX_MANAGER_DOCKER_MAINTENANCE_ENABLED=true
SANDBOX_MANAGER_DOCKER_BUILD_CACHE_MAX_USED_SPACE=32gb
SANDBOX_MANAGER_DOCKER_BUILD_CACHE_RESERVED_SPACE=8gb
SANDBOX_MANAGER_DOCKER_BUILD_CACHE_TIMEOUT_SECS=180
```

The cleanup code never selects another environment by image-name prefix alone; the environment ID
label is the authoritative ownership boundary. The reference lookup only exists for images built by
older releases before ownership labels were added.

## Auth

`/health` is public. All other API routes require authentication. The service refuses to start
unless both user authentication and signed internal requests are enabled:

```env
SANDBOX_MANAGER_REQUIRE_AUTH=true
SANDBOX_MANAGER_REQUIRE_SIGNED_INTERNAL_REQUESTS=true
```

The Docker stack binds the backend host port to `127.0.0.1` by default. Set
`SANDBOX_MANAGER_BIND_HOST` only when an external reverse proxy must reach the host port.

Supported callers:

- internal service: caller-specific short-lived signed token
- managed access client: `x-sandbox-client-id` + `x-sandbox-client-key`
- user token: `Authorization: Bearer ...`, verified through `user_service`

Task Runner, Project Service, and MCP Management sign each request with their dedicated
Configuration Center secret. Raw signing secrets are never transmitted to Sandbox Manager.

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
