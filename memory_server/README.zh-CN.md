# Memory Server

## 模块概述

Memory Server 是 Agent Stack 的长期记忆域服务。

它负责保存消息历史、总结、滚动总结和记忆产物，并把原始对话沉淀为可复用的上下文，供后续编排与模型调用使用。

## 这个服务负责什么

- 会话与消息持久化
- 分层总结与滚动总结生成
- 记忆检索与上下文组装
- 面向管理和运维的记忆后台界面

## 为什么需要独立记忆服务

持续运行的 Agent 系统不能长期依赖“把完整历史一直塞给模型”这种方式。

这个服务的存在，是为了从结构上解决几个问题：
- 随着历史增长，控制 prompt 成本
- 保留关键事实、决策、风险和待办
- 支持多层次总结，而不是只有一份扁平摘要
- 让记忆质量、任务执行情况和总结策略具备可观测性

## 目录结构

- `backend/`：Rust 记忆服务
- `frontend/`：React 管理台
- `shared/`：共享契约与通用资源

## 后端快速启动

```bash
cd backend
cp .env.example .env
cargo run --bin memory_server
```

默认后端地址：
- `http://localhost:7080`

## 前端快速启动

```bash
cd frontend
npm install
npm run dev
```

默认前端地址：
- `http://localhost:5176`

## 整体联调启动

在仓库根目录执行：

```bash
./restart_services.sh restart
```
