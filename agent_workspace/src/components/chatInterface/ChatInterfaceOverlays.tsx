import type { ComponentProps } from 'react';

import AiModelManager from '../AiModelManager';
import ApplicationsPanel from '../ApplicationsPanel';
import McpManager from '../McpManager';
import NotepadPanel from '../NotepadPanel';
import UserSettingsPanel from '../UserSettingsPanel';
import UiPromptHistoryDrawer from './UiPromptHistoryDrawer';

interface ChatInterfaceOverlaysProps {
  uiPromptHistoryProps: ComponentProps<typeof UiPromptHistoryDrawer>;
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

export default function ChatInterfaceOverlays({
  uiPromptHistoryProps,
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

      {showMcpManager && (
        <McpManager onClose={() => setShowMcpManager(false)} />
      )}

      <NotepadPanel
        isOpen={showNotepadPanel}
        onClose={() => setShowNotepadPanel(false)}
      />

      {showAiModelManager && (
        <AiModelManager onClose={() => setShowAiModelManager(false)} />
      )}

      {showUserSettings && (
        <UserSettingsPanel onClose={() => setShowUserSettings(false)} />
      )}

      <ApplicationsPanel
        isOpen={showApplicationsPanel}
        onClose={() => setShowApplicationsPanel(false)}
        title="应用列表"
        layout="modal"
      />
    </>
  );
}
