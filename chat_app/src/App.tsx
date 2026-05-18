import { useEffect } from 'react';
import { ChatInterface } from './components/ChatInterface';
import { AuthPanel } from './components/AuthPanel';
import { useTheme } from './hooks/useTheme';
import { ChatStoreProvider } from './lib/store/ChatStoreContext';
import { useAuthStore } from './lib/auth/authStore';
import { ErrorBoundary } from './components/ErrorBoundary';
import { DialogProvider } from './components/ui/DialogProvider';
import { RealtimeProvider } from './lib/realtime/RealtimeProvider';
import { I18nProvider, useI18n } from './i18n/I18nProvider';
import './styles/index.css';

interface AppProps {
  projectId?: string;
}

function AppShell({ projectId }: AppProps = {}) {
  const { actualTheme } = useTheme();
  const { user, initialized, bootstrap, accessToken } = useAuthStore();
  const { t } = useI18n();

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
        {t('app.initializing')}
      </div>
    );
  }

  if (!user?.id) {
    return <AuthPanel />;
  }

  return (
    <ErrorBoundary>
      <RealtimeProvider accessToken={accessToken}>
        <DialogProvider>
          <ChatStoreProvider userId={user.id} projectId={projectId}>
            <div className="App">
              <ChatInterface />
            </div>
          </ChatStoreProvider>
        </DialogProvider>
      </RealtimeProvider>
    </ErrorBoundary>
  );
}

function App({ projectId }: AppProps = {}) {
  return (
    <I18nProvider>
      <AppShell projectId={projectId} />
    </I18nProvider>
  );
}

export default App;
