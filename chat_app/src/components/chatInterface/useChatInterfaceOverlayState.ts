import { useState } from 'react';

export const useChatInterfaceOverlayState = () => {
  const [showAiModelManager, setShowAiModelManager] = useState(false);
  const [showMemoryModelSettings, setShowMemoryModelSettings] = useState(false);
  const [showTaskModelSettings, setShowTaskModelSettings] = useState(false);
  const [showAgentManager, setShowAgentManager] = useState(false);
  const [showSystemContextEditor, setShowSystemContextEditor] = useState(false);
  const [showApplicationsPanel, setShowApplicationsPanel] = useState(false);
  const [showNotepadPanel, setShowNotepadPanel] = useState(false);
  const [showUserSettings, setShowUserSettings] = useState(false);

  return {
    showAiModelManager,
    setShowAiModelManager,
    showMemoryModelSettings,
    setShowMemoryModelSettings,
    showTaskModelSettings,
    setShowTaskModelSettings,
    showAgentManager,
    setShowAgentManager,
    showSystemContextEditor,
    setShowSystemContextEditor,
    showApplicationsPanel,
    setShowApplicationsPanel,
    showNotepadPanel,
    setShowNotepadPanel,
    showUserSettings,
    setShowUserSettings,
  };
};
