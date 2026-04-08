# agent_orchestrator（主后端）

## 项目定位
`agent_orchestrator` 是整个 agent stack 的主编排后端。
它负责会话、消息、工具路由和模型流式输出，是前端工程化 AI 工作流的核心执行层。

## 这个子项目解决什么问题
AI 后端在工程场景常见痛点：
- 业务逻辑、模型调用、工具调用混在一起，链路脆弱，
- 多轮上下文编排不稳定，
- 模型与工具并行执行时问题难以定位。

该服务通过统一编排与协议处理，把复杂执行链路变成可控、可观测的主流程。

## 核心优势
1. 编排优先
- 将对话主流程与记忆域、网关层职责解耦。

2. 支持实时交互
- 天然支持流式响应与工具协同执行。

3. Rust 生产特性
- 基于 Axum + Tokio，兼顾性能与运行稳定性。

4. 方便整套联动
- 可与记忆服务、前端一起一键拉起本地环境。

## 技术栈
- Rust（Axum + Tokio）
- SQLx（SQLite）
- MongoDB 客户端支持

## 本地运行（开发）
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
