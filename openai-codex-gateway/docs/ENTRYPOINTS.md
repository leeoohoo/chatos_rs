# Gateway Entrypoints Index

## Service Startup

- User-facing command: `python openai-codex-gateway/server.py --host 127.0.0.1 --port 8089`
- Control wrapper: `openai-codex-gateway/gateway_ctl.sh`
- Control implementation: `openai-codex-gateway/scripts/gateway_ctl.sh`

## Test Entrypoints

- Stable test runner wrapper: `openai-codex-gateway/tests/run_by_case.sh`
- Test runner implementation: `openai-codex-gateway/tests/scripts/run_by_case.sh`
- Makefile aliases: `openai-codex-gateway/Makefile`

## Test Target Map

- `all`: gateway unit regression (`test_gateway_*.py`)
- `stream`: `tests/case_stream`
- `http`: `tests/case_http`
- `request`: `tests/case_request`
- `create-response`: `tests/case_create_response`
- `response`: `tests/case_response`
- `base`: `tests/case_base`
- `core`: `tests/case_core`
- `integration`: `tests/case_integration`
- `mcp-request` / `mcp-stream` / `mcp-full`: MCP integration scripts
- `function-single` / `function-multi` / `function-stream`: function-call integration scripts

## Compatibility Policy

- Wrapper paths (`gateway_ctl.sh`, `tests/run_by_case.sh`) are stable public entrypoints.
- Internal implementations are classified under `scripts/` and `tests/scripts/` for maintainability.
