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

## Python 依赖
安装依赖：

```bash
python -m pip install -r requirements.txt
```

## 启动
在当前目录执行：

```bash
python server.py --host 127.0.0.1 --port 8088
```

## 说明
- 默认优先使用 `vendor/` 下内置 SDK。
- 如需强制使用当前环境已安装 SDK：

```bash
export CODEX_GATEWAY_SDK_MODE=installed
```
