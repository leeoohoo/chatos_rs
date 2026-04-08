# memory_server

## 项目定位
`memory_server` 是 agent stack 的记忆域服务。
它负责通过总结、再总结、记忆检索和运维能力来管理长期上下文。

## 这个子项目解决什么问题
如果没有独立记忆层，AI 系统常见问题是：
- 原始历史不断回放导致 token 成本失控，
- 跨会话连续性差，
- 定时任务下总结重复或冲突，
- 记忆质量缺乏运维可见性。

`memory_server` 通过结构化记忆流水线、定时沉淀机制和管理台能力解决这些问题。

## 核心优势
1. 分层记忆生命周期
- 支持会话总结、再总结、回忆型记忆抽取。

2. 成本与质量平衡
- 在压缩上下文成本的同时保留关键事实、决策和待办。

3. 任务执行一致性
- 面向定时流水线设计，支持锁与幂等思路，降低重复处理风险。

4. 自带运维能力
- 提供管理后台便于查看、校验和维护记忆数据。

## 目录结构
- `backend/`：Rust 记忆服务
- `frontend/`：React 管理台

## 后端快速启动
```bash
cd backend
cp .env.example .env
cargo run --bin memory_server
```

默认后端地址：
- `http://localhost:7080`

常用 Mongo 环境变量：
- `MEMORY_SERVER_MONGODB_URI`
- `MEMORY_SERVER_MONGODB_DATABASE`

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
