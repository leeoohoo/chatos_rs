# openai-codex-gateway

## 项目定位
`openai-codex-gateway` 提供一个兼容 OpenAI 协议的 HTTP 网关，
用于把现有 OpenAI 风格客户端低成本接入到本系统。

## 这个子项目解决什么问题
接入自定义 AI 后端时，常见问题包括：
- 客户端和工具链被单一协议实现绑定，
- 迁移到新后端时改造成本高，
- 上游协议与内部服务接口不一致。

该网关通过协议适配层统一入口，在保留兼容性的同时保持后端实现灵活。

## 核心优势
1. OpenAI 协议兼容
- 提供常见模型查询与响应生成接口。

2. 降低迁移成本
- 复用已有 SDK、客户端和自动化工具。

3. 部署方式灵活
- 支持内置 SDK 模式与环境已安装 SDK 模式。

4. 职责边界清晰
- 将协议转换从核心业务服务中解耦。

## 主要接口
- `GET /healthz`
- `GET /v1/models`
- `POST /v1/responses`

## 代码目录分层
- `server.py`：网关入口（标准库 HTTP Server）
- `gateway_base/`：基础通用能力（日志、策略、类型、traceback 等）
- `gateway_core/`：运行时核心能力（runtime、sdk loader、state store）
- `gateway_http/`：HTTP 输入输出与路由
- `gateway_request/`：请求解析与 payload 归一化
- `gateway_response/`：响应对象构建
- `gateway_stream/`：流式编排与 SSE 事件流
- `create_response/`：create-response 解析与执行模块
- `tests/`：按域拆分的测试目录
- `docs/ENTRYPOINTS.md`：启动/测试入口索引

## 测试目录分层
- `tests/case_stream/`
- `tests/case_http/`
- `tests/case_request/`
- `tests/case_create_response/`
- `tests/case_response/`
- `tests/case_base/`
- `tests/case_core/`
- `tests/case_integration/`

为什么使用 `case_*` 命名：
- 避免与 `http`、`create_response` 等模块名/标准库包同名，降低 `unittest` 发现与导入冲突风险。

## 推荐测试命令
在仓库根目录执行：

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

也可以用快捷脚本：

```bash
bash openai-codex-gateway/tests/run_by_case.sh all
bash openai-codex-gateway/tests/run_by_case.sh stream
bash openai-codex-gateway/tests/run_by_case.sh mcp-request
```

测试脚本目录说明：
- 兼容入口：`tests/run_by_case.sh`
- 具体实现：`tests/scripts/run_by_case.sh`

如果你已经在 `openai-codex-gateway/` 目录，也可以用 Makefile 别名：

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

## 模型传参说明
- 支持客户端在 `POST /v1/responses` 中传 `model`。
- 网关会把该模型透传给 codex app-server。
- 如果不传 `model`，会使用 codex 当前默认模型。

## Python 依赖
安装依赖：

```bash
python -m pip install -r requirements.txt
```

## 启动
在当前目录执行：

```bash
python server.py --host 127.0.0.1 --port 8089
```

## 快速后台启动（推荐）

```bash
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

## 说明
- 默认优先使用 `vendor/` 下内置 SDK。
- 如需强制使用当前环境已安装 SDK：

```bash
export CODEX_GATEWAY_SDK_MODE=installed
```
