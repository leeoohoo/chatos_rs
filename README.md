# Chatos RS

Chatos RS is an AI platform for engineering workflows. It combines conversational collaboration, tool orchestration, task execution, sandboxed runtimes, and long-term memory in one system.

中文文档: [README.zh-CN.md](./README.zh-CN.md)
Install guide: [INSTALL_GUIDE.zh-CN.md](./INSTALL_GUIDE.zh-CN.md)

## Architecture

- `chatos/`: main Chatos service, with `frontend/` and `backend/`
- `user_service/`: identity, user, and delegation-token service
- `harness/`: independent upstream Harness checkout, used as the code hosting / DevOps microservice
- `task_runner_service/`: cloud task execution service
- `memory_engine/`: long-term memory service
- `project_management_service/`: project and task metadata service
- `sandbox_manager_service/`: Docker-backed sandbox manager
- `local_connector_service/`: cloud relay service for local connector clients
- `local_connector_client/`: host-side connector client, still run outside Docker
- `db_connection_hub/`: database connection hub
- `official_website_service/`: public website service

## Docker Quick Start

All cloud/server services now run through Docker Compose. From the repository root:

```bash
cp docker/.env.example docker/.env
# edit docker/.env for external secrets/API keys; internal service tokens have defaults
docker/deploy.sh up
```

`docker/deploy.sh up` pulls prebuilt images from GHCR by default; the default Compose file is image-only and does not build from source. To build images from local source instead:

```bash
docker/deploy.sh dev
```

Equivalent Make target:

```bash
make docker-up
```

For local source builds:

```bash
make dev
```

Useful commands:

```bash
docker/deploy.sh ps
docker/deploy.sh logs
docker/deploy.sh restart
docker/deploy.sh down
docker/deploy.sh reset
```

Default URLs:

- Main app: `http://localhost:8088`
- Main backend: `http://localhost:3997`
- Harness: `http://localhost:3000`
- User Service: `http://localhost:39191`
- Memory Engine: `http://localhost:4178`
- Task Runner: `http://localhost:39091`
- Project Management: `http://localhost:39211`
- Sandbox Manager: `http://localhost:8096`
- Local Connector Service: `http://localhost:39230`
- DB Connection Hub: `http://localhost:5174`
- Official Website: `http://localhost:39251`

## Local Connector Client

The cloud relay service is containerized, but `local_connector_client/` intentionally remains host-side because it needs access to the user's local workspace and local Docker runtime.

```bash
make local-connector-client
make local-connector-client-status
make local-connector-client-stop
```

Host-side connector defaults live in root `.env.example`. Docker stack defaults live in `docker/.env.example`.

## Harness Source

Harness is checked out at `harness/` from `https://github.com/leeoohoo/harness.git` and keeps its own Git history. The parent Chatos repository ignores that directory, so update it with normal Git commands inside `harness/`.

Fresh workspaces can recreate the source checkout with:

```bash
git clone https://github.com/leeoohoo/harness.git harness
```

The Docker stack runs Harness from the `harness/harness` image and stores data in the `harness-data` volume.

## Sandbox Docker Control

`sandbox_manager_service` runs inside the Compose stack and controls the host Docker daemon through `/var/run/docker.sock`. Sandbox containers join the same Docker bridge network as the Compose services, so the manager can call sandbox agents by container name instead of publishing each agent port.

This is intentional for the cloud stack, but it is a privileged deployment model: a container with access to the Docker socket effectively has host Docker administration privileges.

## Checks

```bash
make smoke
make test
```

`make smoke` validates repository guardrails and Docker Compose configuration.

## CI Images

GitHub Actions builds and pushes the Chatos service images to GHCR on `main`, `master`, and version tags. The default image namespace is `ghcr.io/leeoohoo`, configured in `docker/.env.example` as:

```env
CHATOS_IMAGE_NAMESPACE=ghcr.io/leeoohoo
CHATOS_IMAGE_TAG=latest
```

Set `CHATOS_IMAGE_TAG=sha-<commit>` to deploy a specific CI build.

If the GHCR packages are not public, run `docker login ghcr.io` on the deployment machine before `docker/deploy.sh up`.

Local source builds are isolated in `docker/compose.build.yml`; CI validates both the image-only runtime Compose file and the local-build overlay.

## Third-Party Notices

See [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).
