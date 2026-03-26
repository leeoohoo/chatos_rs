# openai-codex-gateway

## Positioning
`openai-codex-gateway` provides an OpenAI-compatible HTTP gateway for Codex app-server integration.
It lets existing OpenAI-style clients connect to this stack with minimal changes.

## What It Solves
When integrating custom AI backends, teams often face:
- client lock-in to one protocol shape,
- expensive migrations for SDKs/tools,
- incompatibility between upstream and internal service interfaces.

This gateway normalizes API access so compatibility is preserved while backend implementations stay flexible.

## Core Advantages
1. OpenAI-compatible interface
- Exposes familiar endpoints for model listing and response generation.

2. Lower migration effort
- Reuses existing clients and toolchains built for OpenAI APIs.

3. Deployment flexibility
- Supports bundled SDK mode and installed SDK mode.

4. Clean system boundary
- Keeps protocol adaptation concerns out of core business services.

## Main Endpoints
- `GET /healthz`
- `GET /v1/models`
- `POST /v1/responses`

## Python Dependencies
Install dependencies:

```bash
python -m pip install -r requirements.txt
```

## Run
From this directory:

```bash
python server.py --host 127.0.0.1 --port 8088
```

## Notes
- The gateway prefers bundled SDK code under `vendor/` by default.
- You can force installed SDK mode via:

```bash
export CODEX_GATEWAY_SDK_MODE=installed
```
