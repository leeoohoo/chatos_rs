// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Suspense, lazy, type ComponentProps, type ReactNode } from 'react';

import TurnRuntimeContextDrawer from './TurnRuntimeContextDrawer';
import { useI18n } from '../../i18n/I18nProvider';

const AgentManager = lazy(() => import('../AgentManager'));
const ApplicationsPanel = lazy(() => import('../ApplicationsPanel'));
const NotepadPanel = lazy(() => import('../NotepadPanel'));
const UserSettingsPanel = lazy(() => import('../UserSettingsPanel'));

interface ChatInterfaceOverlaysProps {
  runtimeContextProps: ComponentProps<typeof TurnRuntimeContextDrawer>;
  showNotepadPanel: boolean;
  setShowNotepadPanel: (value: boolean) => void;
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
