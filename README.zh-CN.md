# Chatos RS

## 项目定位
`Chatos RS` 是一个面向开发与工程协作场景的 AI 平台。  
它把对话交互、工具调用、长期记忆、以及 OpenAI 兼容接入统一到一套系统中，目标是让 AI 能稳定地“持续工作”，而不是只做一次性聊天。

## 这个项目解决什么问题
传统聊天式 AI 在工程场景常见问题：
- 上下文只在当前会话内有效，跨会话信息容易丢失
- 历史越长 token 成本越高，推理效率下降
- 工具链路分散，接入和维护成本高
- 外部系统接入协议不统一

`Chatos RS` 的设计就是为了解决这些问题：  
通过“主对话服务 + 记忆服务 + 网关层”实现持续上下文、成本控制和可集成性。

## 核心优势
1. 长期记忆能力
- 支持会话总结、再总结、记忆沉淀，保留跨会话的关键事实、决策和待办。

2. 上下文成本可控
- 通过分层总结与定时任务压缩上下文，减少无效 token 消耗，同时保持连续性。

3. 工具协作友好
- 支持工具调用与 MCP 场景，适合接入工程工作流与外部能力。

4. 架构可扩展
- 前端、主后端、记忆服务、网关解耦，支持独立部署与水平扩展。

5. 生态兼容性强
- 提供 OpenAI 兼容接口，已有客户端和 SDK 可低成本接入。

## 架构分层
- `chat_app/`：主前端交互层
- `chat_app_server_rs/`：主后端编排层（会话、消息、工具、流式响应）
- `memory_server/`：记忆域（总结、再总结、记忆检索、管理台）
- `openai-codex-gateway/`：OpenAI 兼容网关层

## 本地一键启动
在仓库根目录执行：

```bash
./restart_services.sh restart
```

常用命令：

```bash
./restart_services.sh status
./restart_services.sh stop
```

默认日志路径：
- `logs/backend.log`
- `logs/frontend.log`
- `logs/memory_backend.log`
- `logs/memory_frontend.log`

## 开发方案归档
方案/评估/契约文档统一收纳在：
- 本地目录 `docs/plans/`（该目录已配置不上传 git）

## 子项目文档
- [chat_app English](./chat_app/README.en.md)
- [chat_app 中文](./chat_app/README.zh-CN.md)
- [chat_app_server_rs English](./chat_app_server_rs/README.en.md)
- [chat_app_server_rs 中文](./chat_app_server_rs/README.zh-CN.md)
- [memory_server English](./memory_server/README.en.md)
- [memory_server 中文](./memory_server/README.zh-CN.md)
- [openai-codex-gateway English](./openai-codex-gateway/README.en.md)
- [openai-codex-gateway 中文](./openai-codex-gateway/README.zh-CN.md)

## 开源协议
本项目使用 [MIT License](./LICENSE)。
