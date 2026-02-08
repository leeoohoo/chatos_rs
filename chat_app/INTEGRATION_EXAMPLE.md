# 集成示例 Integration Examples

演示如何在不同类型项目中集成 `@leeoohoo/aichat`。How to integrate the library into various setups.

## 1) 新 React 项目 New React App

创建项目 Create project

```bash
npm create vite@latest my-chat-app -- --template react-ts
cd my-chat-app
npm install

# 安装组件 Install component
npm install @leeoohoo/aichat

# 安装 Tailwind（可选） Tailwind (optional)
npm install -D tailwindcss postcss autoprefixer
npx tailwindcss init -p
```

Tailwind 配置 Tailwind config (`tailwind.config.js`)

```js
/** @type {import('tailwindcss').Config} */
export default {
  content: [
    './index.html',
    './src/**/*.{js,ts,jsx,tsx}',
    './node_modules/@leeoohoo/aichat/dist/**/*.{js,ts,jsx,tsx}',
  ],
  theme: { extend: {} },
  plugins: [],
}
```

`src/index.css`

```css
@tailwind base;
@tailwind components;
@tailwind utilities;
```

在 App 中使用 Use in App (`src/App.tsx`)

```tsx
import StandaloneChatInterface from '@leeoohoo/aichat';
import '@leeoohoo/aichat/styles';
import './App.css';

export default function App() {
  return (
    <div className="h-screen w-full bg-gray-50">
      <div className="container mx-auto h-full max-w-6xl">
        <StandaloneChatInterface className="h-full" />
      </div>
    </div>
  );
}
```

## 2) 在现有项目中 Existing Project

页面集成 As a page

```tsx
import { StandaloneChatInterface } from '@leeoohoo/aichat';
import '@leeoohoo/aichat/styles';

export default function ChatPage() {
  return (
    <div className="min-h-screen bg-gray-100">
      <header className="bg-white shadow-sm border-b">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <h1 className="text-2xl font-bold py-4">AI 助手</h1>
        </div>
      </header>

      <main className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="bg-white rounded-lg shadow h-[600px]">
          <StandaloneChatInterface className="h-full" />
        </div>
      </main>
    </div>
  );
}
```

模态框集成 As a modal

```tsx
import { useState } from 'react';
import { StandaloneChatInterface } from '@leeoohoo/aichat';
import '@leeoohoo/aichat/styles';

export default function ChatModal() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        onClick={() => setOpen(true)}
        className="fixed bottom-4 right-4 bg-blue-600 text-white p-4 rounded-full shadow hover:bg-blue-700"
      >
        Chat
      </button>

      {open && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="absolute inset-0 bg-black/50" onClick={() => setOpen(false)} />
          <div className="relative bg-white rounded-lg shadow-xl w-full max-w-4xl h-[80vh] m-4">
            <div className="flex justify-between items-center p-4 border-b">
              <h2 className="text-lg font-semibold">AI 助手</h2>
              <button onClick={() => setOpen(false)} className="text-gray-500 hover:text-gray-700">✕</button>
            </div>
            <div className="h-[calc(80vh-4rem)]">
              <StandaloneChatInterface className="h-full" />
            </div>
          </div>
        </div>
      )}
    </>
  );
}
```

## 3) 自定义状态管理 Advanced: custom state

```tsx
import { useEffect } from 'react';
import {
  ChatInterface,
  MessageList,
  InputArea,
  SessionList,
  useChatStore,
  ThemeToggle,
} from '@leeoohoo/aichat';
import '@leeoohoo/aichat/styles';

export default function CustomChatApp() {
  const { sessions, currentSession, messages, createSession, switchSession, sendMessage, isLoading } = useChatStore();

  useEffect(() => {
    if (sessions.length === 0) createSession('默认对话');
  }, [sessions.length, createSession]);

  const handleSendMessage = async (content: string) => {
    if (!currentSession) return;
    try {
      await sendMessage({ content, sessionId: currentSession.id, role: 'user' });
    } catch (e) {
      console.error('发送消息失败:', e);
    }
  };

  return (
    <div className="h-screen flex bg-gray-100">
      <div className="w-80 bg-white border-r flex flex-col">
        <div className="p-4 border-b">
          <div className="flex justify-between items-center mb-4">
            <h1 className="text-xl font-bold">AI 助手</h1>
            <ThemeToggle />
          </div>
          <button onClick={() => createSession('新对话')} className="w-full bg-blue-600 text-white py-2 px-4 rounded">
            + 新建对话
          </button>
        </div>
        <div className="flex-1 overflow-hidden">
          <SessionList sessions={sessions} currentSessionId={currentSession?.id} onSessionSelect={switchSession} />
        </div>
      </div>

      <div className="flex-1 flex flex-col">
        {currentSession ? (
          <>
            <div className="bg-white border-b p-4">
              <h2 className="text-lg font-semibold">{currentSession.title}</h2>
            </div>
            <div className="flex-1 overflow-hidden">
              <MessageList messages={messages} isLoading={isLoading} />
            </div>
            <div className="bg-white border-t">
              <InputArea onSendMessage={handleSendMessage} disabled={isLoading} />
            </div>
          </>
        ) : (
          <div className="flex-1 flex items-center justify-center text-gray-500">请选择或创建一个对话</div>
        )}
      </div>
    </div>
  );
}
```

## 4) 环境配置 Env

环境变量 `.env.local`（前端可选） Frontend optional

```env
VITE_OPENAI_API_KEY=your_openai_api_key_here
VITE_OPENAI_BASE_URL=https://api.openai.com/v1
VITE_API_BASE_URL=http://localhost:3001
```

后端（推荐） Backend (recommended): 使用本仓库服务器 Use the bundled server

```bash
cd server/chat_app_node_server
npm install
cp .env.example .env  # 填 OPENAI_API_KEY
npm start             # http://localhost:3001
```

## 5) 部署建议 Deployment

- 纯前端可用，但持久化/多会话等需后端 API; Static-only works, persistence requires backend
- 生产注意配置 API 密钥与 CORS; Properly set API keys and CORS in production
- 样式必须包含组件 CSS; Ensure Tailwind scans node_modules and import styles

## 故障排除 Troubleshooting

- 无样式 No styles: 确认导入 `@leeoohoo/aichat/styles`
- Tailwind 未生效: 检查 `tailwind.config.js` 的 `content`
- TS 报错: 确保类型依赖安装正确
- API 失败: 检查 `.env`、后端是否可达、浏览器网络

调试 Debug helper

```tsx
import { useChatStore } from '@leeoohoo/aichat';

const DebugInfo = () => {
  const store = useChatStore();
  if (process.env.NODE_ENV === 'development') {
    console.log('Chat Store State:', {
      sessions: store.sessions,
      currentSession: store.currentSession,
      messages: store.messages,
      isLoading: store.isLoading,
    });
  }
  return null;
};
```
