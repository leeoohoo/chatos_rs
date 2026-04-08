# Contact Task Service

## 模块概述

Contact Task Service 是 Agent Stack 的任务平台。

它的职责不是“临时保存一条任务消息”，而是把任务真正作为系统中的一等对象来管理，包括状态流转、依赖关系、调度执行和结果沉淀。

## 这个服务负责什么

- 任务持久化
- 任务状态与生命周期管理
- 面向执行器的任务接口与调度能力
- 面向用户和运维的任务管理界面

## 为什么需要独立任务平台

如果任务只是聊天过程中的附属物，就很难做好这些事情：
- 明确区分待确认、待执行、执行中、暂停、失败、完成等状态
- 表达任务之间的依赖和验证关系
- 让定时执行器稳定获取“当前真正该执行的任务”
- 沉淀任务结果、执行记录和异常信息

独立任务平台的价值，就是让任务链路真正闭环。

## 目录结构

- `backend/`：Rust 任务服务
- `frontend/`：React 任务管理台

## 后端快速启动

```bash
cd backend
cargo run --bin contact_task_service
```

## 前端快速启动

```bash
cd frontend
npm install
npm run dev
```
