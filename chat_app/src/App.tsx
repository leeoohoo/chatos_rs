import { useEffect } from 'react';
import { ChatInterface } from './components/ChatInterface';
import { AuthPanel } from './components/AuthPanel';
import { useTheme } from './hooks/useTheme';
import { ChatStoreProvider } from './lib/store/ChatStoreContext';
import { useAuthStore } from './lib/auth/authStore';
import { ErrorBoundary } from './components/ErrorBoundary';
import './styles/index.css';

interface AppProps {
  projectId?: string;
}

function App({ projectId }: AppProps = {}) {
  const { actualTheme } = useTheme();
  const { user, initialized, bootstrap } = useAuthStore();

  // 确保主题正确应用
  useEffect(() => {
    document.documentElement.classList.remove('light', 'dark');
    document.documentElement.classList.add(actualTheme);
  }, [actualTheme]);

  useEffect(() => {
    bootstrap();
  }, [bootstrap]);

  if (!initialized) {
    return (
      <div className="min-h-screen flex items-center justify-center text-gray-500">
        正在初始化...
      </div>
    );
  }

  if (!user?.id) {
    return <AuthPanel />;
  }

  return (
    <ErrorBoundary>
      <ChatStoreProvider userId={user.id} projectId={projectId}>
        <div className="App">
          <ChatInterface />
        </div>
      </ChatStoreProvider>
    </ErrorBoundary>
  );
}

export default App;
