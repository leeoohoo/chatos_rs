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

默认日志与 PID：
- `/tmp/chatos_rs_dev/codex_gateway.log`
- `/tmp/chatos_rs_dev/codex_gateway.pid`

## 说明
- 默认优先使用 `vendor/` 下内置 SDK。
- 如需强制使用当前环境已安装 SDK：

```bash
export CODEX_GATEWAY_SDK_MODE=installed
```
