# @leeoohoo/aichat

React AI Chat Component and Node server. Bilingual README (中文/English).

—

一个功能完整的 React AI 聊天组件库，配套 Node 后端服务，支持会话管理、MCP 工具集成与持久化存储。

## 功能 Features

- 开箱即用 Standalone chat UI, drop-in usage
- 会话管理 Sessions CRUD, multi-session switching
- AI 模型配置 AI model configs (OpenAI-compatible)
- MCP 集成 Model Context Protocol tool calls (HTTP/STDIO)
- 数据持久化 SQLite by default (MongoDB optional in server)
- 主题切换 Light/Dark/Auto, responsive UI
- 类型安全 Full TypeScript types

## 代码结构 Repository Structure

- 根目录 Root: React 组件库 React component library (Vite + TS)
- server/chat_app_node_server: Node API 服务 Node API server (Express, SSE, MCP, SQLite/MongoDB)
- examples/complete-example: 集成示例 Example app (web/Electron)

## 快速开始 Quick Start

1) 作为依赖使用 Use as a dependency

```bash
npm install @leeoohoo/aichat
# or
yarn add @leeoohoo/aichat
# or
pnpm add @leeoohoo/aichat
```

最简用法 Minimal usage

```tsx
import StandaloneChatInterface from '@leeoohoo/aichat';
import '@leeoohoo/aichat/styles';

export default function App() {
  return <StandaloneChatInterface className="h-screen" />;
}
```

2) 本仓库本地开发 Run locally (this repo)

- 启动后端 Start the API server

```bash
cd server/chat_app_node_server
npm install
cp .env.example .env   # 设置 OPENAI_API_KEY 等 set your OPENAI_API_KEY
npm start              # http://localhost:3001
```

- 启动前端 Start the frontend demo

```bash
cd /path/to/chat_app
npm install
npm run dev            # http://localhost:5173 (proxy /api → 3001)
```

生产环境可通过组件传入 `apiBaseUrl` 或 `port` 覆盖后端地址。
In production, pass `apiBaseUrl` or `port` to the component to point at your server.

```tsx
<StandaloneChatInterface apiBaseUrl="https://your.domain/api" />
// or
<StandaloneChatInterface port={3001} />
```

## 使用 Usage

推荐 Recommended: 独立组件 Standalone component

```tsx
import StandaloneChatInterface from '@leeoohoo/aichat';
import '@leeoohoo/aichat/styles';

<StandaloneChatInterface
  className="h-full"
  userId="user_123"
  projectId="proj_456"
  showMcpManager
  showAiModelManager
  showSystemContextEditor
  showAgentManager
/>
```

可选：类方式 API Optional: class-style API

```tsx
import { AiChat } from '@leeoohoo/aichat';

const aiChat = new AiChat(
  'user_123',
  'proj_456',
  '/api',           // configUrl (API base)
  'h-full',         // className
  true, true, true, // showMcpManager, showAiModelManager, showSystemContextEditor
  true              // showAgentManager
);

export default function App() {
  return aiChat.render();
}
```

常用导出 Common exports

- 组件 Components: `StandaloneChatInterface`, `ChatInterface`, `MessageList`, `InputArea`, `SessionList`, `ThemeToggle`, `McpManager`, `AiModelManager`, `SystemContextEditor`
- Hooks: `useTheme`, `useChatStore`
- 工具 Utils/API: `lib/api`, `lib/services`, `lib/utils`
- 类型 Types: 从 `@leeoohoo/aichat` 导入类型 Import types from the package

## 环境与配置 Env & Config

前端 Frontend

- 开发环境 dev: Vite 代理将 `/api` 指向 `http://localhost:3001`
- 生产 prod: 通过 `apiBaseUrl`/`port` 或自行配置反向代理 Configure `apiBaseUrl`/`port` or reverse proxy

后端 Server (`server/chat_app_node_server`)

- `.env`

```env
OPENAI_API_KEY=your_api_key
OPENAI_BASE_URL=https://api.openai.com/v1
PORT=3001
NODE_ENV=development
```

- 数据库 Database: 默认 SQLite，配置见 `server/chat_app_node_server/config/database.json`；可切换 MongoDB

## API 概览 API Overview

- Sessions: `/api/sessions` CRUD, messages: `/api/sessions/:id/messages`
- Agents: `/api/agents` CRUD, stream: `/api/agents/chat/stream`
- MCP: `/api/mcp-configs` CRUD, resource reading
- AI Model Configs: `/api/ai-model-configs` CRUD
- System Contexts: `/api/system-contexts` CRUD, active context
- Streaming chat: `/api/agent_v2/chat/stream` (SSE)

详细见 See details: `server/chat_app_node_server/README.md`

## 示例 Examples

- `examples/complete-example`: 完整示例（含 Electron） Complete sample (with Electron)

```bash
cd examples/complete-example
npm install
npm run dev:full   # 前端 + 后端 Frontend + Server
```

## 开发脚本 Scripts (root)

- `npm run dev` Vite 本地预览 Local dev
- `npm run build:lib` 构建库 Build library bundle
- `npm run test`/`test:ui` 单测 Vitest
- `npm run lint`/`lint:fix` ESLint
- `npm run storybook` 组件调试 Storybook

## 许可证 License

MIT

—

如需更细的接入与排障，参见 Also see: `USAGE.md`, `INTEGRATION_EXAMPLE.md`, `MODULE_CONTROL.md`。

## 样式定制

组件使用Tailwind CSS构建，你可以通过以下方式定制样式：

1. **覆盖CSS变量**

```css
:root {
  --chat-primary-color: #your-color;
  --chat-background-color: #your-background;
}
```

2. **使用自定义CSS类**

```tsx
<ChatInterface className="my-custom-chat" />
```

3. **主题定制**

```css
.dark {
  --chat-background: #1a1a1a;
  --chat-text: #ffffff;
}

.light {
  --chat-background: #ffffff;
  --chat-text: #000000;
}
```

## 类型定义

包含完整的TypeScript类型定义：

```tsx
import type {
  Message,
  Session,
  Attachment,
  ToolCall,
  ChatConfig,
  AiModelConfig,
  McpConfig,
  Theme,
  ChatInterfaceProps,
  MessageListProps,
  InputAreaProps,
  SessionListProps,
} from '@leeoohoo/aichat';
```

## 许可证

MIT

## 贡献

欢迎提交Issue和Pull Request！

## 支持

如果你觉得这个项目有用，请给它一个⭐️！
