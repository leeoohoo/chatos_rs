# OpenAI Codex Gateway

## Overview

OpenAI Codex Gateway is the protocol compatibility layer of Agent Stack.

It exposes OpenAI-style HTTP APIs while forwarding requests into the internal Codex/App Server runtime, making it easier for existing SDKs and clients to integrate without large migration effort.

## What It Is Responsible For

- OpenAI-compatible HTTP surface
- request translation into internal runtime calls
- streaming and non-streaming response handling
- request-level MCP/tool compatibility bridging

## Why It Matters

This gateway keeps compatibility concerns out of the core orchestration layer.

That allows the system to:
- integrate with existing clients faster
- preserve familiar SDK usage patterns
- isolate protocol translation from core business logic
- evolve the internal stack without breaking every caller

## Main Endpoints

- `GET /healthz`
- `GET /v1/models`
- `POST /v1/responses`

## Install Dependencies

```bash
python -m pip install -r requirements.txt
```

## Start

In this directory:

```bash
python server.py --host 127.0.0.1 --port 8089
```

## Background Control

```bash
./gateway_ctl.sh start
./gateway_ctl.sh status
./gateway_ctl.sh tail
./gateway_ctl.sh restart
./gateway_ctl.sh stop
```
