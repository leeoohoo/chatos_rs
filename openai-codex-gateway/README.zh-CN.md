# OpenAI Codex Gateway

## 模块概述

OpenAI Codex Gateway 是 Agent Stack 的协议兼容层。

它对外暴露 OpenAI 风格的 HTTP API，对内把请求转发到 Codex / App Server 运行时，使外部系统可以在不理解内部实现细节的情况下完成接入。

## 这个网关负责什么

- 提供 OpenAI 兼容接口
- 把外部请求翻译为内部运行时调用
- 支持普通响应与流式响应
- 提供请求级 MCP / 工具声明的兼容桥接

## 为什么需要它

很多现有客户端和自动化系统，天然都是围绕 OpenAI 风格接口构建的。

这个网关的价值就在于：
- 不需要让所有调用方直接学习内部服务契约
- 复用已有 SDK、脚本和工具链
- 把协议适配从核心业务编排中解耦
- 降低迁移与实验成本

## 主要接口

- `GET /healthz`
- `GET /v1/models`
- `POST /v1/responses`

## Python 依赖

```bash
python -m pip install -r requirements.txt
```

## 启动

在当前目录执行：

```bash
python server.py --host 127.0.0.1 --port 8089
```

## 后台控制

```bash
./gateway_ctl.sh start
./gateway_ctl.sh status
./gateway_ctl.sh tail
./gateway_ctl.sh restart
./gateway_ctl.sh stop
```

默认日志文件：
- `logs/codex_gateway.log`
- `logs/codex_gateway.pid`
