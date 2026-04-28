import { Suspense, lazy, type ComponentProps } from 'react';

import TurnRuntimeContextDrawer from './TurnRuntimeContextDrawer';
import UiPromptHistoryDrawer from './UiPromptHistoryDrawer';

const AiModelManager = lazy(() => import('../AiModelManager'));
const ApplicationsPanel = lazy(() => import('../ApplicationsPanel'));
const McpManager = lazy(() => import('../McpManager'));
const NotepadPanel = lazy(() => import('../NotepadPanel'));
const UserSettingsPanel = lazy(() => import('../UserSettingsPanel'));

interface ChatInterfaceOverlaysProps {
  uiPromptHistoryProps: ComponentProps<typeof UiPromptHistoryDrawer>;
  runtimeContextProps: ComponentProps<typeof TurnRuntimeContextDrawer>;
  showMcpManager: boolean;
  setShowMcpManager: (value: boolean) => void;
  showNotepadPanel: boolean;
  setShowNotepadPanel: (value: boolean) => void;
  showAiModelManager: boolean;
  setShowAiModelManager: (value: boolean) => void;
  showUserSettings: boolean;
  setShowUserSettings: (value: boolean) => void;
  showApplicationsPanel: boolean;
  setShowApplicationsPanel: (value: boolean) => void;
}

const OverlayFallback = () => null;

export default function ChatInterfaceOverlays({
  uiPromptHistoryProps,
  runtimeContextProps,
  showMcpManager,
  setShowMcpManager,
  showNotepadPanel,
  setShowNotepadPanel,
  showAiModelManager,
  setShowAiModelManager,
  showUserSettings,
  setShowUserSettings,
  showApplicationsPanel,
  setShowApplicationsPanel,
}: ChatInterfaceOverlaysProps) {
  return (
    <>
      <UiPromptHistoryDrawer {...uiPromptHistoryProps} />
      <TurnRuntimeContextDrawer {...runtimeContextProps} />

      {showMcpManager && (
        <Suspense fallback={<OverlayFallback />}>
          <McpManager onClose={() => setShowMcpManager(false)} />
        </Suspense>
      )}

      {showNotepadPanel && (
        <Suspense fallback={<OverlayFallback />}>
          <NotepadPanel
            isOpen={showNotepadPanel}
            onClose={() => setShowNotepadPanel(false)}
          />
        </Suspense>
      )}

      {showAiModelManager && (
        <Suspense fallback={<OverlayFallback />}>
          <AiModelManager onClose={() => setShowAiModelManager(false)} />
        </Suspense>
      )}

      {showUserSettings && (
        <Suspense fallback={<OverlayFallback />}>
          <UserSettingsPanel onClose={() => setShowUserSettings(false)} />
        </Suspense>
      )}

      {showApplicationsPanel && (
        <Suspense fallback={<OverlayFallback />}>
          <ApplicationsPanel
            isOpen={showApplicationsPanel}
            onClose={() => setShowApplicationsPanel(false)}
            title="应用列表"
            layout="modal"
          />
        </Suspense>
      )}
    </>
  );
}
