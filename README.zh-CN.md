# Agent Stack

## 项目概述

Agent Stack 是一个面向工程协作、长期上下文管理与异步任务执行的 AI Agent 基础设施项目。

它的核心目标，是把 AI 从“一次性聊天工具”升级为“可以持续工作的系统能力”：用户可以和 AI 联系人对话，AI 可以基于上下文规划并创建结构化任务，任务可以在受控工具权限下异步执行，结果再通过统一的交互链路回传给用户。

## 项目使命

这个项目围绕一个很明确的方向展开：

AI 不应该只负责回答问题，还应该能够参与真实工作。

要做到这一点，系统必须同时具备这些能力：
- 跨会话持续保留上下文，而不是只记住当前聊天窗口
- 把任务规划显式化，而不是全部隐藏在模型内部
- 让工具调用有边界、有权限、有可观测性
- 支持异步执行，而不是所有交互都必须等任务完成
- 提供兼容接口，便于外部系统低成本接入

## 这个项目解决什么问题

传统以聊天为中心的 AI 系统，在工程与运营场景中通常会遇到这些问题：
- 历史上下文越来越长，token 成本不断升高
- 重要事实、决策、偏好和待办很容易在后续对话中丢失
- 工具链路缺乏统一编排，难以复用、审计与维护
- 任务创建、确认、执行、回传往往分散在不同模块里，难以形成闭环
- 外部系统接入时，经常需要额外做一层协议适配

Agent Stack 通过模块化服务架构来解决这些问题，把工作空间交互、编排、记忆、任务生命周期、IM 传输和兼容网关分层管理，但又通过明确的运行时契约把它们串成一个整体。

## 核心能力

- 持久化记忆：通过分层总结、滚动总结和记忆沉淀，在不重复注入完整历史的前提下保留关键信息
- IM 化协作：用户面对的是联系人式消息交互，而不是原始工具流
- 结构化任务规划：可以把自然语言需求转成可确认、可执行、可追踪的任务与任务图
- 异步任务执行：任务确认后在后台持续运行，完成后再把结果回推给用户
- 受控工具运行时：执行阶段可按范围使用内置能力，例如 `read`、`write`、`terminal`、`remote`、`notepad`、`ui_prompter`
- OpenAI 兼容接入：可通过熟悉的 API 形态与外部 SDK、客户端集成

## 端到端工作链路

1. 用户在工作空间里向某个 AI 联系人发送消息。
2. 编排层从聊天记录、总结、记忆以及当前授权资源中组装运行上下文。
3. 模型根据上下文决定直接回复，或生成一个或多个待确认任务。
4. 用户确认后，任务平台在后台使用所需工具与资源执行这些任务。
5. 执行结果、阶段总结和后续可复用知识会回写到系统中，并通过 IM 链路再发送给用户。

这套链路特别适合“需要规划、执行、验证和长期记忆”的工程场景，而不只是即时问答。

## 仓库结构

- `agent_workspace/`：前端工作空间，承载联系人对话、任务交互和操作界面
- `agent_orchestrator/`：主后端编排层，负责对话流转、工具调用、任务规划与执行协调
- `memory_server/`：记忆服务，负责总结、滚动总结、记忆检索和记忆管理
- `im_service/`：面向 IM 的消息投递层，负责用户与联系人之间的异步消息传输
- `contact_task_service/`：任务平台，负责任务持久化、调度、生命周期状态与执行侧任务接口
- `openai-codex-gateway/`：OpenAI 兼容网关，方便外部客户端和 SDK 接入

## 为什么要这样分层

这个系统有意采用按领域拆分的方式，而不是把所有能力堆在一个服务里：
- 工作空间层专注用户体验与交互模型
- 编排层专注模型行为、Prompt、工具和运行策略
- 记忆层专注总结策略、检索质量和长期知识沉淀
- 任务平台专注生命周期、调度、依赖关系和可观测性
- IM 层专注异步消息语义与回推链路
- 网关层专注协议兼容与对外接入

正是这种分层，才让平台既能支撑“像聊天一样自然的协作体验”，又能支撑“像系统一样稳定的后台执行”。

## 快速开始

在仓库根目录执行：

```bash
./restart_services.sh restart
```

常用命令：

```bash
./restart_services.sh status
./restart_services.sh stop
```

默认运行日志写入 `logs/` 目录，例如：
- `logs/backend.log`
- `logs/frontend.log`
- `logs/memory_backend.log`
- `logs/memory_frontend.log`

## 更多文档

- [English README](./README.en.md)
- [双语概览](./README.md)

子项目文档：
- [agent_workspace English](./agent_workspace/README.en.md)
- [agent_workspace 中文](./agent_workspace/README.zh-CN.md)
- [agent_orchestrator English](./agent_orchestrator/README.en.md)
- [agent_orchestrator 中文](./agent_orchestrator/README.zh-CN.md)
- [im_service English](./im_service/README.en.md)
- [im_service 中文](./im_service/README.zh-CN.md)
- [contact_task_service English](./contact_task_service/README.en.md)
- [contact_task_service 中文](./contact_task_service/README.zh-CN.md)
- [memory_server English](./memory_server/README.en.md)
- [memory_server 中文](./memory_server/README.zh-CN.md)
- [openai-codex-gateway English](./openai-codex-gateway/README.en.md)
- [openai-codex-gateway 中文](./openai-codex-gateway/README.zh-CN.md)

## 方案归档

历史方案与实现笔记统一放在本地 `docs/plans/` 目录中，该目录默认不纳入 git 跟踪。

## 开源协议

本项目使用 [MIT License](./LICENSE)。
