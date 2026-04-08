# Agent Workspace

## 模块概述

Agent Workspace 是 Agent Stack 面向用户的主前端工作空间。

它不是一个普通的聊天页面，而是整套系统的统一交互入口：用户在这里与 AI 联系人对话、查看任务确认卡片、接收后台任务完成结果，并以更接近 IM 的方式推动工作持续进行。

## 这个模块负责什么

- 联系人式对话与主工作空间交互
- 任务创建确认、状态查看与执行结果呈现
- 通过 HTTP 与 WebSocket 对接后端能力
- 向用户展示上下文、系统反馈和关键执行信息
- 作为整个 Agent Stack 的主要操作入口

## 为什么它重要

这个前端承载的不是“把模型输出渲染出来”这么简单，而是要支撑一种更贴近真实协作的工作模式：
- 用户面对的是联系人式沟通，而不是原始工具流
- 任务创建后需要确认，而不是悄悄在后台发生
- 长任务需要在后台继续运行，而不是阻塞当前消息轮次
- 任务完成后应该异步回传，而不是要求用户一直盯着流式输出
- 多个后端服务需要在体验上看起来像一个完整产品

Agent Workspace 的作用，就是把这些能力组织成一个对用户友好的产品界面。

## 技术栈

- React 18
- TypeScript
- Vite
- Zustand

## 本地开发

在当前目录执行：

```bash
npm install
npm run dev
```

## 构建

```bash
npm run build
```

## 常用脚本

- `npm run dev`
- `npm run build`
- `npm run preview`
- `npm run type-check`
- `npm run test`
- `npm run lint`

## 整体联调启动

在仓库根目录执行：

```bash
./restart_services.sh restart
```

## 更多文档

- [English README](./README.en.md)
- [使用说明](./USAGE.md)
