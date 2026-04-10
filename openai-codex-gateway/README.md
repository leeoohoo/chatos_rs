# OpenAI-Compatible Codex Gateway

这个目录提供一个可直接运行的 HTTP 网关：

- 对外提供 OpenAI 风格接口：`/v1/responses`、`/v1/models`
- 内部通过本仓库 `sdk/python` 调用 `codex app-server`

## 目录

- `server.py`：网关服务入口（标准库 HTTP Server）
- `gateway_base/`：基础通用能力（日志、策略、类型、工具、traceback）
- `gateway_core/`：运行时核心能力（runtime、sdk_loader、state_store）
- `gateway_http/`：HTTP 输入输出与路由
- `gateway_request/`：请求解析与 payload 处理
- `gateway_response/`：响应对象构建
- `gateway_stream/`：流式响应与事件编排
- `create_response/`：create-response 相关解析与执行编排
- `tests/`：测试目录，按 `case_*` 域拆分
- `docs/ENTRYPOINTS.md`：网关启动/测试入口索引

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
python openai-codex-gateway/server.py --host 127.0.0.1 --port 8089
```

快速后台启动（推荐）：

```bash
cd openai-codex-gateway
./gateway_ctl.sh start
```

常用命令：

```bash
./gateway_ctl.sh status
./gateway_ctl.sh tail
./gateway_ctl.sh restart
./gateway_ctl.sh stop
```

脚本目录说明：

- 兼容入口保留在根目录：`./gateway_ctl.sh`
- 具体实现位于：`scripts/gateway_ctl.sh`

默认日志与 PID：

- `/tmp/chatos_rs_dev/codex_gateway.log`
- `/tmp/chatos_rs_dev/codex_gateway.pid`

可选参数：

- `--codex-bin`：指定 codex 可执行文件路径
- `--cwd`：app-server 工作目录（默认仓库根目录）
- `--sandbox`：`read-only` / `workspace-write` / `danger-full-access`（默认 `read-only`）

说明：

- 若不传 `--codex-bin`，网关会自动尝试使用 `PATH` 里的 `codex`。
- 启动时会打印 `[gateway.state] sdk source=...`，可用于确认当前实际使用的是 `bundled` 还是 `installed` SDK。
- 如果仍提示找不到 runtime，请显式指定：

```bash
python openai-codex-gateway/server.py --codex-bin "$(which codex)" --port 8089
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

模型说明：

- 可以在每次 `POST /v1/responses` 请求里传 `model`（例如 `gpt-5`）。
- 网关会把该 `model` 透传给 codex app-server（新会话和续聊都会生效）。
- 如果不传 `model`，则使用 codex 当前默认模型（响应中会显示为 `codex-default`）。

## OpenAI Python SDK 调用示例

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8089/v1",
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

client = OpenAI(base_url="http://127.0.0.1:8089/v1", api_key="dummy")

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

测试目录分层（`openai-codex-gateway/tests`）：

- `case_stream/`：流式主链路与编排逻辑
- `case_http/`：HTTP 层 IO 与路由
- `case_request/`：请求解析与 payload 处理
- `case_create_response/`：create-response 解析与执行
- `case_response/`：响应构建
- `case_base/`：基础能力（traceback 等）
- `case_core/`：运行时核心能力
- `case_integration/`：端到端/会话级集成脚本

命名说明：

- 测试子目录统一使用 `case_*` 前缀，避免与 `http`、`create_response` 等模块名或标准库包同名，导致 `unittest` 导入冲突。

推荐执行命令（在仓库根目录运行，默认调用 `http://127.0.0.1:8089/v1`）：

```bash
# 全量回归（推荐）
python -m unittest discover -s openai-codex-gateway/tests -p 'test_gateway_*.py'

# 按域回归
python -m unittest discover -s openai-codex-gateway/tests/case_stream -p 'test_gateway_*.py'
python -m unittest discover -s openai-codex-gateway/tests/case_http -p 'test_gateway_*.py'
python -m unittest discover -s openai-codex-gateway/tests/case_request -p 'test_gateway_*.py'
python -m unittest discover -s openai-codex-gateway/tests/case_create_response -p 'test_gateway_*.py'
python -m unittest discover -s openai-codex-gateway/tests/case_response -p 'test_gateway_*.py'
python -m unittest discover -s openai-codex-gateway/tests/case_base -p 'test_gateway_*.py'
python -m unittest discover -s openai-codex-gateway/tests/case_core -p 'test_gateway_*.py'
```

也可以使用分类快捷脚本：

```bash
bash openai-codex-gateway/tests/run_by_case.sh all
bash openai-codex-gateway/tests/run_by_case.sh stream
bash openai-codex-gateway/tests/run_by_case.sh mcp-request
```

测试脚本目录说明：

- 兼容入口：`tests/run_by_case.sh`
- 具体实现：`tests/scripts/run_by_case.sh`

如果你已经在 `openai-codex-gateway/` 目录，也可以直接用 Makefile 别名：

```bash
make test
make test-stream
make test-mcp-request
```

常用集成脚本：

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

可选环境变量：

- `GATEWAY_BASE_URL`：默认 `http://127.0.0.1:8089/v1`
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

可用验证脚本：`tests/case_integration/test_mcp_tools_request.py`

这个脚本会启动一个本地最小 MCP stdio server（`tests/fixtures/mcp_secret_server.py`），
然后通过请求级 `tools` 验证网关可成功调用 MCP 工具并返回结果。

完整会话验证（两轮续聊 + 每轮请求级 MCP）：

```bash
python openai-codex-gateway/tests/case_integration/test_mcp_tools_full_session.py
```

流式验证（请求级 MCP + SSE）：

```bash
python openai-codex-gateway/tests/case_integration/test_mcp_tools_stream.py
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

- `tests/case_integration/test_function_tools_single.py`：单工具调用循环
- `tests/case_integration/test_function_tools_multi_call.py`：多工具调用循环（包含同轮多个 `function_call` 的处理逻辑）
- `tests/case_integration/test_function_tools_stream.py`：流式 function-call 循环（首轮流式拿 call，第二轮流式回传输出）

## 完整调用模板（OpenAI SDK）

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8089/v1",
    api_key="dummy-key",
)

# 1) 首轮
resp1 = client.responses.create(
    model="gpt-5",
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
    model="gpt-5",
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
