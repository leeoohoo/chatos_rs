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

## Code Layout
- `server.py`: gateway entrypoint (stdlib HTTP server)
- `gateway_base/`: shared base utilities (logging, policy, types, traceback helpers)
- `gateway_core/`: runtime/core modules (runtime, sdk loader, state store)
- `gateway_http/`: HTTP IO and routing
- `gateway_request/`: request parsing and payload normalization
- `gateway_response/`: response object builders
- `gateway_stream/`: streaming orchestration and SSE/event flow
- `create_response/`: create-response parser/runner modules
- `tests/`: domain-classified test suites
- `docs/ENTRYPOINTS.md`: startup/test entrypoint index

## Test Layout
- `tests/case_stream/`
- `tests/case_http/`
- `tests/case_request/`
- `tests/case_create_response/`
- `tests/case_response/`
- `tests/case_base/`
- `tests/case_core/`
- `tests/case_integration/`

Why `case_*` naming:
- avoids import/module collisions with names like `http` and `create_response` during `unittest` discovery.

## Recommended Test Commands
Run from repository root:

```bash
# full regression (recommended)
python -m unittest discover -s openai-codex-gateway/tests -p 'test_gateway_*.py'

# per-domain
python -m unittest discover -s openai-codex-gateway/tests/case_stream -p 'test_gateway_*.py'
python -m unittest discover -s openai-codex-gateway/tests/case_http -p 'test_gateway_*.py'
python -m unittest discover -s openai-codex-gateway/tests/case_request -p 'test_gateway_*.py'
python -m unittest discover -s openai-codex-gateway/tests/case_create_response -p 'test_gateway_*.py'
python -m unittest discover -s openai-codex-gateway/tests/case_response -p 'test_gateway_*.py'
python -m unittest discover -s openai-codex-gateway/tests/case_base -p 'test_gateway_*.py'
python -m unittest discover -s openai-codex-gateway/tests/case_core -p 'test_gateway_*.py'
```

Or use the shortcut runner:

```bash
bash openai-codex-gateway/tests/run_by_case.sh all
bash openai-codex-gateway/tests/run_by_case.sh stream
bash openai-codex-gateway/tests/run_by_case.sh mcp-request
```

Test runner layout note:
- Stable entrypoint: `tests/run_by_case.sh`
- Implementation path: `tests/scripts/run_by_case.sh`

If you are already in `openai-codex-gateway/`, you can use Makefile aliases:

```bash
make test
make test-stream
make test-mcp-request
```

Common integration scripts:

```bash
python openai-codex-gateway/tests/case_integration/test_single_turn.py
python openai-codex-gateway/tests/case_integration/test_continuous_session.py
python openai-codex-gateway/tests/case_integration/test_long_conversation.py
python openai-codex-gateway/tests/case_integration/test_mcp_tools_request.py
python openai-codex-gateway/tests/case_integration/test_mcp_tools_stream.py
python openai-codex-gateway/tests/case_integration/test_function_tools_single.py
python openai-codex-gateway/tests/case_integration/test_function_tools_multi_call.py
python openai-codex-gateway/tests/case_integration/test_function_tools_stream.py
```

## Model Selection
- Clients can pass `model` in each `POST /v1/responses` request.
- The gateway forwards that model to codex app-server.
- If `model` is omitted, codex default model is used.

## Python Dependencies
Install dependencies:

```bash
python -m pip install -r requirements.txt
```

## Run
From this directory:

```bash
python server.py --host 127.0.0.1 --port 8089
```

## Quick Background Run (Recommended)

```bash
./gateway_ctl.sh start
```

Common commands:

```bash
./gateway_ctl.sh status
./gateway_ctl.sh tail
./gateway_ctl.sh restart
./gateway_ctl.sh stop
```

Script layout note:
- Backward-compatible entrypoint stays at repo root: `./gateway_ctl.sh`
- Implementation now lives at: `scripts/gateway_ctl.sh`

Default log and PID paths:
- `/tmp/chatos_rs_dev/codex_gateway.log`
- `/tmp/chatos_rs_dev/codex_gateway.pid`

## Notes
- The gateway prefers bundled SDK code under `vendor/` by default.
- You can force installed SDK mode via:

```bash
export CODEX_GATEWAY_SDK_MODE=installed
```
