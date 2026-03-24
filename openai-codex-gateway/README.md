# OpenAI-Compatible Codex Gateway

这个目录提供一个可直接运行的 HTTP 网关：

- 对外提供 OpenAI 风格接口：`/v1/responses`、`/v1/models`
- 内部通过本仓库 `sdk/python` 调用 `codex app-server`

## 目录

- `server.py`：网关服务实现（标准库 HTTP Server）

## 启动前准备

默认情况下，网关会优先使用本目录下 `vendor/` 内置的 SDK 代码，无需额外安装。

如果你想强制使用当前 Python 环境里已安装的官方 SDK，可以设置：

```bash
export CODEX_GATEWAY_SDK_MODE=installed
```

如果你确实需要手动安装 SDK，再执行：

```bash
cd chat_app_server_rs/docs/codex/sdk/python
python -m pip install -e .
```

## 启动服务

在仓库根目录执行：

```bash
python openai-codex-gateway/server.py --host 127.0.0.1 --port 8088
```

可选参数：

- `--codex-bin`：指定 codex 可执行文件路径
- `--cwd`：app-server 工作目录（默认仓库根目录）
- `--sandbox`：`read-only` / `workspace-write` / `danger-full-access`（默认 `read-only`）

说明：

- 若不传 `--codex-bin`，网关会自动尝试使用 `PATH` 里的 `codex`。
- 启动时会打印 `[gateway.state] sdk source=...`，可用于确认当前实际使用的是 `bundled` 还是 `installed` SDK。
- 如果仍提示找不到 runtime，请显式指定：

```bash
python openai-codex-gateway/server.py --codex-bin "$(which codex)" --port 8088
```

## 接口

### 1) 健康检查

```http
GET /healthz
```

### 2) 模型列表

```http
GET /v1/models
Authorization: Bearer <API_KEY>   # 可选
```

### 3) 生成响应

```http
POST /v1/responses
Content-Type: application/json
Authorization: Bearer <API_KEY>   # 可选

{
  "model": "gpt-5",
  "input": "用一句话介绍 Rust",
  "stream": false
}
```

支持流式：`"stream": true`（SSE）。

支持续聊：传 `previous_response_id`，网关会把它映射到同一个 Codex thread。

额外支持：

- `cwd`：请求级工作目录。网关会把它传给 `thread_start/thread_resume/turn_start`，
  用来把本轮以及后续续聊绑定到指定项目目录，而不是服务启动时的默认 `--cwd`。

## OpenAI Python SDK 调用示例

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8088/v1",
    api_key="dummy-or-real-key",
)

resp = client.responses.create(
    model="gpt-5",
    input="写一句 hello",
)
print(resp.output_text)
```

流式：

```python
from openai import OpenAI

client = OpenAI(base_url="http://127.0.0.1:8088/v1", api_key="dummy")

stream = client.responses.stream(
    model="gpt-5",
    input="给我三条 Rust 学习建议",
)
for event in stream:
    print(event)
```

## 安全默认值

- 默认 `sandbox=read-only`
- 网关不会自动批准命令执行/文件改动审批请求（返回 `decline`）

## 测试脚本

先安装 OpenAI Python SDK：

```bash
python -m pip install openai
```

然后执行（默认调用 `http://127.0.0.1:8088/v1`）：

```bash
python openai-codex-gateway/tests/test_single_turn.py
python openai-codex-gateway/tests/test_continuous_session.py
python openai-codex-gateway/tests/test_long_conversation.py
python openai-codex-gateway/tests/test_mcp_tools_request.py
python openai-codex-gateway/tests/test_mcp_tools_stream.py
python openai-codex-gateway/tests/test_function_tools_single.py
python openai-codex-gateway/tests/test_function_tools_multi_call.py
python openai-codex-gateway/tests/test_function_tools_stream.py
```

可选环境变量：

- `GATEWAY_BASE_URL`：默认 `http://127.0.0.1:8088/v1`
- `GATEWAY_API_KEY`：默认 `dummy-key`
- `GATEWAY_TEST_MODEL`：可选；指定模型名（不传就用网关默认模型）

## MCP tools 说明

网关现在支持在单次 `POST /v1/responses` 请求里直接传 `tools: [{"type": "mcp", ...}]`，
并自动映射成 thread 级别的 `config.mcp_servers`。

默认行为：当请求里包含 `type: "mcp"` 的工具时，网关会固定只保留你请求里声明的 MCP server，
并关闭 Apps/Plugins/Connectors 注入的 MCP 来源。

已支持字段（`tools[].type == "mcp"`）：

- `server_label`（必填）
- HTTP 模式（二选一）：
  `server_url`（或 `url`）+ 可选 `bearer_token_env_var` / `headers` / `env_headers`
- stdio 模式（二选一）：
  `command` + 可选 `args` / `env` / `env_vars` / `cwd`
- 可选工具过滤：`enabled_tools`（或 `allowed_tools`）、`disabled_tools`（或 `blocked_tools`）
- 可选：`required`（bool）

注意：

- 不能传 inline `bearer_token`；请使用 `bearer_token_env_var`。
- 每个请求里的 `server_label` 不能重复。

可用验证脚本：`tests/test_mcp_tools_request.py`

这个脚本会启动一个本地最小 MCP stdio server（`tests/fixtures/mcp_secret_server.py`），
然后通过请求级 `tools` 验证网关可成功调用 MCP 工具并返回结果。

完整会话验证（两轮续聊 + 每轮请求级 MCP）：

```bash
python openai-codex-gateway/tests/test_mcp_tools_full_session.py
```

流式验证（请求级 MCP + SSE）：

```bash
python openai-codex-gateway/tests/test_mcp_tools_stream.py
```

## 函数工具（客户端本地执行，不服务化 MCP）

如果你的客户端不是 MCP server，而是“模型返回要调用哪个工具 -> 客户端直接执行本地方法 -> 再把结果回传”，
可以用 OpenAI Responses 的 function-call 循环：

1. 首轮请求传 `tools: [{"type":"function", ...}]`
2. 网关返回 `output[].type == "function_call"`（可能一次返回多个）
3. 客户端执行每个工具调用，拼装 `function_call_output`
4. 带上 `previous_response_id` 发下一轮，直到返回普通 `message`

说明：当前 function-call 循环已支持流式请求（`stream=true`）。

示例请求体（首轮）：

```json
{
  "input": "请调用 read_runtime_secret 并返回 secret",
  "tools": [
    {
      "type": "function",
      "name": "read_runtime_secret",
      "description": "Read secret from local app",
      "parameters": {
        "type": "object",
        "properties": {
          "nonce": { "type": "string" }
        },
        "required": ["nonce"]
      }
    }
  ]
}
```

如果返回了多个 `function_call`，客户端应一次性把所有结果都回传，例如：

```json
{
  "previous_response_id": "resp_xxx",
  "input": [
    {
      "type": "function_call_output",
      "call_id": "call_a",
      "output": "{\"alpha\":\"...\"}"
    },
    {
      "type": "function_call_output",
      "call_id": "call_b",
      "output": "{\"beta\":\"...\"}"
    }
  ]
}
```

可运行脚本：

- `tests/test_function_tools_single.py`：单工具调用循环
- `tests/test_function_tools_multi_call.py`：多工具调用循环（包含同轮多个 `function_call` 的处理逻辑）
- `tests/test_function_tools_stream.py`：流式 function-call 循环（首轮流式拿 call，第二轮流式回传输出）

## 完整调用模板（OpenAI SDK）

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8089/v1",
    api_key="dummy-key",
)

# 1) 首轮
resp1 = client.responses.create(
    input="请调用 mcp__my_mcp__my_tool 并只输出结果",
    tools=[{
        "type": "mcp",
        "server_label": "my_mcp",
        "command": "/usr/bin/python3",      # 或 server_url/url（HTTP 模式）
        "args": ["/abs/path/to/your_mcp_server.py"],
        "env": {"MY_TOKEN": "xxx"},
    }],
)

print(resp1.id, resp1.output_text)

# 2) 续聊（同会话）
resp2 = client.responses.create(
    previous_response_id=resp1.id,
    input="继续调用 mcp__my_mcp__my_tool，返回最新结果",
    tools=[{
        "type": "mcp",
        "server_label": "my_mcp",
        "command": "/usr/bin/python3",
        "args": ["/abs/path/to/your_mcp_server.py"],
    }],
)

print(resp2.id, resp2.output_text)
```
