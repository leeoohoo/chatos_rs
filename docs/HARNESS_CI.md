# Harness CI Build And Deploy

This repository mirrors the current worktree into the server-side Harness repo
and uses local Docker images tagged as `ghcr.io/leeoohoo/*:harness-ci`.

## Pipelines

- `chatos-rs-images`: builds all Chat OS service images and deploys the whole stack.
- `image-<service>`: builds one service image and deploys only that service.

Buildable service names come from:

```bash
bash docker/deploy.sh build-services
```

Examples:

- `image-user-service-backend`
- `image-chatos-frontend`
- `image-task-runner-backend`
- `image-official-website-frontend`

`image-sandbox-agent-image` only builds and verifies the sandbox agent image,
because it is not a long-running Compose service.

## Refresh Harness Pipelines

From the repository root:

```bash
HARNESS_ADMIN_PASSWORD='<password>' bash scripts/build-images-on-harness.sh
```

The script:

1. Generates `.harness/pipelines/images/image-*.yml`.
2. Pushes the current worktree snapshot to Harness.
3. Creates or updates the `chatos-rs-images` pipeline.
4. Creates or updates every `image-<service>` pipeline.
5. Runs `chatos-rs-images` unless `HARNESS_CI_RUN=false`.

To refresh the pipeline list without running a build:

```bash
HARNESS_CI_RUN=false HARNESS_ADMIN_PASSWORD='<password>' bash scripts/build-images-on-harness.sh
```

## Partial Build From CLI

To build and deploy one service from CLI:

```bash
CHATOS_CI_IMAGE_SERVICES=user-service-backend \
HARNESS_ADMIN_PASSWORD='<password>' \
bash scripts/build-images-on-harness.sh
```

For normal use, run the corresponding `image-<service>` pipeline directly in Harness.
