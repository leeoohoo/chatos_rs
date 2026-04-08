# OpenAI Codex Gateway

OpenAI Codex Gateway is the protocol compatibility layer of Agent Stack.

It exposes OpenAI-style HTTP endpoints on the outside while forwarding requests into the internal Codex/App Server runtime, allowing existing SDKs, tools, and clients to integrate with the system at low migration cost.

OpenAI Codex Gateway 是 Agent Stack 的协议兼容层。

它对外提供 OpenAI 风格的 HTTP 接口，对内把请求转发到 Codex / App Server 运行时，从而让现有 SDK、工具链和客户端可以以较低改造成本接入系统。

## What This Gateway Does

- Exposes OpenAI-compatible endpoints such as `/v1/models` and `/v1/responses`
- Maps OpenAI-style requests to internal runtime execution
- Supports both normal and streaming response flows
- Provides a bridge for MCP-style tool declarations inside request payloads

## Why It Exists

Most teams already have clients built around OpenAI-shaped APIs.

This gateway exists to:
- avoid forcing every client to learn internal service contracts
- preserve compatibility with existing SDKs and tools
- isolate protocol adaptation from core orchestration logic
- make migration and experimentation cheaper

## Main Endpoints

- `GET /healthz`
- `GET /v1/models`
- `POST /v1/responses`

## Python Dependencies

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

Default log files:
- `logs/codex_gateway.log`
- `logs/codex_gateway.pid`

## More Docs

- [中文说明](./README.zh-CN.md)
- [English README](./README.en.md)
