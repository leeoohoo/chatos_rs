import { Suspense, lazy, type ComponentProps, type ReactNode } from 'react';

import TurnRuntimeContextDrawer from './TurnRuntimeContextDrawer';
import { useI18n } from '../../i18n/I18nProvider';

const AiModelManager = lazy(() => import('../AiModelManager'));
const AgentManager = lazy(() => import('../AgentManager'));
const ApplicationsPanel = lazy(() => import('../ApplicationsPanel'));
const MemoryModelSettingsPanel = lazy(() => import('../MemoryModelSettingsPanel'));
const NotepadPanel = lazy(() => import('../NotepadPanel'));
const TaskModelSettingsPanel = lazy(() => import('../TaskModelSettingsPanel'));
const TaskRunnerExternalMcpManager = lazy(() => import('../TaskRunnerExternalMcpManager'));
const UserSettingsPanel = lazy(() => import('../UserSettingsPanel'));

interface ChatInterfaceOverlaysProps {
  runtimeContextProps: ComponentProps<typeof TurnRuntimeContextDrawer>;
  showNotepadPanel: boolean;
  setShowNotepadPanel: (value: boolean) => void;
  showAiModelManager: boolean;
  setShowAiModelManager: (value: boolean) => void;
  showMemoryModelSettings: boolean;
  setShowMemoryModelSettings: (value: boolean) => void;
  showTaskModelSettings: boolean;
  setShowTaskModelSettings: (value: boolean) => void;
  showTaskRunnerExternalMcpManager: boolean;
  setShowTaskRunnerExternalMcpManager: (value: boolean) => void;
  showAgentManager: boolean;
  setShowAgentManager: (value: boolean) => void;
  showUserSettings: boolean;
  setShowUserSettings: (value: boolean) => void;
  showApplicationsPanel: boolean;
  setShowApplicationsPanel: (value: boolean) => void;
}

const OverlayFallback = () => null;

interface LazyOverlayProps {
  children: ReactNode;
  open: boolean;
}

const LazyOverlay = ({ children, open }: LazyOverlayProps) => {
  if (!open) {
    return null;
  }
  return (
    <Suspense fallback={<OverlayFallback />}>
      {children}
    </Suspense>
  );
};

export default function ChatInterfaceOverlays({
  runtimeContextProps,
  showNotepadPanel,
  setShowNotepadPanel,
  showAiModelManager,
  setShowAiModelManager,
  showMemoryModelSettings,
  setShowMemoryModelSettings,
  showTaskModelSettings,
  setShowTaskModelSettings,
  showTaskRunnerExternalMcpManager,
  setShowTaskRunnerExternalMcpManager,
  showAgentManager,
  setShowAgentManager,
  showUserSettings,
  setShowUserSettings,
  showApplicationsPanel,
  setShowApplicationsPanel,
}: ChatInterfaceOverlaysProps) {
  const { t } = useI18n();

  return (
    <>
      <TurnRuntimeContextDrawer {...runtimeContextProps} />

      <LazyOverlay open={showNotepadPanel}>
        <NotepadPanel
          isOpen={showNotepadPanel}
          onClose={() => setShowNotepadPanel(false)}
        />
      </LazyOverlay>

      <LazyOverlay open={showAiModelManager}>
        <AiModelManager onClose={() => setShowAiModelManager(false)} />
      </LazyOverlay>

      <LazyOverlay open={showMemoryModelSettings}>
        <MemoryModelSettingsPanel onClose={() => setShowMemoryModelSettings(false)} />
      </LazyOverlay>

      <LazyOverlay open={showTaskModelSettings}>
        <TaskModelSettingsPanel onClose={() => setShowTaskModelSettings(false)} />
      </LazyOverlay>

      <LazyOverlay open={showTaskRunnerExternalMcpManager}>
        <TaskRunnerExternalMcpManager onClose={() => setShowTaskRunnerExternalMcpManager(false)} />
      </LazyOverlay>

      <LazyOverlay open={showAgentManager}>
        <AgentManager onClose={() => setShowAgentManager(false)} />
      </LazyOverlay>

      <LazyOverlay open={showUserSettings}>
        <UserSettingsPanel onClose={() => setShowUserSettings(false)} />
      </LazyOverlay>

      <LazyOverlay open={showApplicationsPanel}>
        <ApplicationsPanel
          isOpen={showApplicationsPanel}
          onClose={() => setShowApplicationsPanel(false)}
          title={t('applications.title')}
          layout="modal"
        />
      </LazyOverlay>
    </>
  );
}
