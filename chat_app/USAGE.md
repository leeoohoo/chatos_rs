# 使用指南 Usage Guide

本库提供完整的 React AI 聊天组件，含会话管理、MCP 协议工具与持久化支持。This library ships a full React AI chat UI with sessions, MCP tools, and persistence.

## 安装 Install

```bash
npm install @leeoohoo/aichat
# or
yarn add @leeoohoo/aichat
# or
pnpm add @leeoohoo/aichat
```

## 基本使用 Basic Usage

1) 最简单方式（推荐） Simplest (recommended)

```tsx
import StandaloneChatInterface from '@leeoohoo/aichat';
import '@leeoohoo/aichat/styles';

export default function App() {
  return (
    <div className="h-screen">
      <StandaloneChatInterface />
    </div>
  );
}
```

2) 具名导入 Named import

```tsx
import { StandaloneChatInterface } from '@leeoohoo/aichat';
import '@leeoohoo/aichat/styles';

export default function App() {
  return (
    <div className="h-screen">
      <StandaloneChatInterface />
    </div>
  );
}
```

3) 高级用法 Advanced

```tsx
import React from 'react';
import {
  ChatInterface,
  useChatStore,
  ThemeToggle
} from '@leeoohoo/aichat';
import '@leeoohoo/aichat/styles';

export default function App() {
  const { sessions, currentSession, createSession, switchSession } = useChatStore();

  return (
    <div className="h-screen flex">
      {/* Sidebar */}
      <div className="w-64 bg-gray-100 p-4">
        <ThemeToggle />
        <button
          onClick={() => createSession('新对话')}
          className="w-full mt-4 p-2 bg-blue-500 text-white rounded"
        >
          新建对话
        </button>

        <div className="mt-4">
          {sessions.map(session => (
            <div
              key={session.id}
              onClick={() => switchSession(session.id)}
              className={`p-2 cursor-pointer rounded ${
                currentSession?.id === session.id ? 'bg-blue-200' : 'hover:bg-gray-200'
              }`}
            >
              {session.title}
            </div>
          ))}
        </div>
      </div>

      {/* Chat area */}
      <div className="flex-1">
        <ChatInterface />
      </div>
    </div>
  );
}
```

## 组件 Components

- 推荐 Recommended: `StandaloneChatInterface`
- 核心 Core: `ChatInterface`, `MessageList`, `InputArea`, `SessionList`, `ThemeToggle`
- 管理 Management: `AiModelManager`, `McpManager`, `SystemContextEditor`
- 工具/通用 Utility: `MarkdownRenderer`, `AttachmentRenderer`, `ToolCallRenderer`, `LoadingSpinner`, `ErrorBoundary`

## Hooks

- `useTheme` 主题 Theme
- `useChatStore` 聊天状态 Chat state

## 样式 Styles

组件使用 Tailwind CSS。引入样式 Import styles:

```tsx
import '@leeoohoo/aichat/styles';
```

## TypeScript 类型 Types

```tsx
import type {
  ChatConfig,
  AiModelConfig,
  McpConfig,
  Theme,
  Message,
  Session
} from '@leeoohoo/aichat';
```

## 注意事项 Notes

- 需要 React 18+ Requires React 18+
- 建议 Tailwind 环境 Recommended Tailwind setup
- 存储/历史等功能需要后端 API Some features require backend API
- 数据库相关仅在 Node 环境 Server-side only for DB features

## 更多示例 More Examples

参见 See `examples/` 目录。
