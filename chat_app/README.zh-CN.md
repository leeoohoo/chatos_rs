# chat_app（前端）

## 项目定位
`chat_app` 是 Chatos RS 的主交互前端。
用户在这里完成 AI 对话、工具触发、执行过程查看，以及跨会话的连续协作。

## 这个子项目解决什么问题
在工程场景中，前端常见问题包括：
- 模型执行时反馈弱，用户不知道系统在做什么，
- 工具调用过程不可见，排障成本高，
- 一次性聊天体验和长期任务体验割裂。

`chat_app` 提供统一的交互层，让用户可以“持续推进工作”，而不只是发一次提问。

## 核心优势
1. 面向工作流的交互
- 针对多轮协作和任务推进设计，而不是仅做问答展示。

2. 连续性更好
- 与后端记忆/上下文能力协同，用户可以自然续接历史任务。

3. 迭代效率高
- 基于 React + Vite + TypeScript，开发反馈快、改动成本低。

4. 便于联调
- 可通过仓库根目录脚本与后端整体联动启动。

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
