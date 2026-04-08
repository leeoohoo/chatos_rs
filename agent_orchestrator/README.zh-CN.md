# Agent Orchestrator

## 模块概述

Agent Orchestrator 是 Agent Stack 的核心后端编排服务。

它负责把“消息、上下文、模型、工具、任务、执行结果”串成一条完整主链路，是整套系统真正意义上的业务中枢。

## 这个服务负责什么

- 接收并处理来自工作空间的聊天请求
- 从消息、总结、记忆和运行时资源中组装模型上下文
- 暴露并路由内置 MCP 与工具能力
- 驱动任务的评审、确认、创建和执行协调
- 把工作空间、记忆服务、IM 服务和任务平台组织成一个统一流程

## 为什么它是系统核心

如果没有单独的编排层，整个平台很容易退化成一堆耦合在一起的 Prompt 逻辑、工具逻辑和传输逻辑。

这个服务存在的意义，就是把这些职责明确拆开：
- 模型行为通过显式流程进行协调
- 工具调用有统一运行时策略
- 任务生命周期被建模为一等概念
- 多服务边界对内清晰、对外连续

## 技术栈

- Rust
- Axum
- Tokio
- SQLx + SQLite
- MongoDB 客户端支持

## 本地运行

在当前目录执行：

```bash
cargo run --bin agent_orchestrator
```

## 构建

```bash
cargo build --release
```

## 基础检查

```bash
cargo check
```

## 整体联调启动

在仓库根目录执行：

```bash
./restart_services.sh restart
```
