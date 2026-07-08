# Chatos RS

Chatos RS is Docker-first for cloud/server deployment. The only component that remains host-side by design is `local_connector_client/`.

## Start The Stack

```bash
cp docker/.env.example docker/.env
# edit docker/.env
docker/deploy.sh up
```

This pulls prebuilt images from GHCR. To build from local source:

```bash
docker/deploy.sh dev
```

Make shortcuts:

```bash
make docker-up
make dev
```

`make docker-up` uses prebuilt images; `make dev` builds local images.

## Commands

```bash
docker/deploy.sh ps
docker/deploy.sh logs
docker/deploy.sh restart
docker/deploy.sh down
docker/deploy.sh reset
```

## Default URLs

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

The Local Connector client is still run on the host because it needs local workspace and local Docker access:

```bash
make local-connector-client
make local-connector-client-status
make local-connector-client-stop
```

## Harness

`harness/` is an independent upstream Git checkout from `https://github.com/leeoohoo/harness.git`; it is ignored by the parent Chatos repository. Fresh workspaces can recreate it with `git clone https://github.com/leeoohoo/harness.git harness`.

Docker Compose runs Harness with the `harness/harness` image.

Open-source attribution is tracked in [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md).

If GHCR packages are not public, run `docker login ghcr.io` before `docker/deploy.sh up`.

## Sandbox Manager

In Docker deployment, Sandbox Manager mounts `/var/run/docker.sock` and creates sandbox containers on the same Compose network. This lets it control the current host Docker daemon from inside the container. Treat this as privileged access.

## Validation

```bash
make smoke
make test
```
