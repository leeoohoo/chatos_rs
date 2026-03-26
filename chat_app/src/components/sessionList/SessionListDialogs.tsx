import React from 'react';

import ConfirmDialog from '../ui/ConfirmDialog';
import { CreateContactModal } from './CreateContactModal';
import { CreateProjectModal, CreateTerminalModal } from './CreateResourceModals';
import { DirPickerDialog, KeyFilePickerDialog } from './Pickers';
import { RemoteConnectionModal } from './RemoteConnectionModal';

interface SessionListDialogsProps {
  createContactModalOpen: boolean;
  agents: any[];
  existingContactAgentIds: string[];
  selectedContactAgentId: string | null;
  contactError: string | null;
  closeCreateSessionModal: () => void;
  setSelectedContactAgentId: (value: string) => void;
  setContactError: (value: string | null) => void;
  handleCreateContactSession: () => Promise<void> | void;

  projectModalOpen: boolean;
  projectRoot: string;
  projectError: string | null;
  setProjectModalOpen: (value: boolean) => void;
  setProjectRoot: (value: string) => void;
  openDirPickerForProject: () => void;
  handleCreateProject: () => Promise<void> | void;

  terminalModalOpen: boolean;
  terminalRoot: string;
  terminalError: string | null;
  setTerminalModalOpen: (value: boolean) => void;
  setTerminalRoot: (value: string) => void;
  openDirPickerForTerminal: () => void;
  handleCreateTerminal: () => Promise<void> | void;

  remoteModalOpen: boolean;
  editingRemoteConnectionId: string | null;
  remoteName: string;
  remoteHost: string;
  remotePort: string;
  remoteUsername: string;
  remoteAuthType: any;
  remotePassword: string;
  remotePrivateKeyPath: string;
  remoteCertificatePath: string;
  remoteDefaultPath: string;
  remoteHostKeyPolicy: any;
  remoteJumpEnabled: boolean;
  remoteJumpHost: string;
  remoteJumpPort: string;
  remoteJumpUsername: string;
  remoteJumpPrivateKeyPath: string;
  remoteJumpPassword: string;
  remoteError: string | null;
  remoteErrorAction: string | null;
  remoteSuccess: string | null;
  remoteTesting: boolean;
  remoteSaving: boolean;
  setRemoteModalOpen: (value: boolean) => void;
  setRemoteName: (value: string) => void;
  setRemoteHost: (value: string) => void;
  setRemotePort: (value: string) => void;
  setRemoteUsername: (value: string) => void;
  setRemoteAuthType: (value: any) => void;
  setRemotePassword: (value: string) => void;
  setRemotePrivateKeyPath: (value: string) => void;
  setRemoteCertificatePath: (value: string) => void;
  setRemoteDefaultPath: (value: string) => void;
  setRemoteHostKeyPolicy: (value: any) => void;
  setRemoteJumpEnabled: (value: boolean) => void;
  setRemoteJumpHost: (value: string) => void;
  setRemoteJumpPort: (value: string) => void;
  setRemoteJumpUsername: (value: string) => void;
  setRemoteJumpPrivateKeyPath: (value: string) => void;
  setRemoteJumpPassword: (value: string) => void;
  openKeyFilePicker: (target: any) => void;
  handleTestRemoteConnection: () => Promise<void> | void;
  handleSaveRemoteConnection: () => Promise<void> | void;

  keyFilePickerOpen: boolean;
  keyFilePickerTitle: string;
  keyFilePickerPath: string | null;
  keyFilePickerParent: string | null;
  keyFilePickerLoading: boolean;
  keyFilePickerItems: any[];
  keyFilePickerError: string | null;
  closeKeyFilePicker: () => void;
  loadKeyFileEntries: (path: string | null) => Promise<void> | void;
  applySelectedKeyFile: (path: string) => void;

  dirPickerOpen: boolean;
  dirPickerTarget: any;
  dirPickerPath: string | null;
  dirPickerParent: string | null;
  dirPickerLoading: boolean;
  dirPickerItems: any[];
  dirPickerError: string | null;
  showHiddenDirs: boolean;
  dirPickerCreateModalOpen: boolean;
  dirPickerNewFolderName: string;
  dirPickerCreatingFolder: boolean;
  closeDirPicker: () => void;
  chooseDir: (path: string) => void;
  openCreateDirModal: () => void;
  setShowHiddenDirs: React.Dispatch<React.SetStateAction<boolean>>;
  loadDirEntries: (path: string | null) => Promise<void> | void;
  setDirPickerCreateModalOpen: (value: boolean) => void;
  setDirPickerNewFolderName: (value: string) => void;
  createDirInPicker: () => Promise<void> | void;

  dialogState: any;
  handleConfirm: () => void;
  handleCancel: () => void;
}

export const SessionListDialogs: React.FC<SessionListDialogsProps> = ({
  createContactModalOpen,
  agents,
  existingContactAgentIds,
  selectedContactAgentId,
  contactError,
  closeCreateSessionModal,
  setSelectedContactAgentId,
  setContactError,
  handleCreateContactSession,
  projectModalOpen,
  projectRoot,
  projectError,
  setProjectModalOpen,
  setProjectRoot,
  openDirPickerForProject,
  handleCreateProject,
  terminalModalOpen,
  terminalRoot,
  terminalError,
  setTerminalModalOpen,
  setTerminalRoot,
  openDirPickerForTerminal,
  handleCreateTerminal,
  remoteModalOpen,
  editingRemoteConnectionId,
  remoteName,
  remoteHost,
  remotePort,
  remoteUsername,
  remoteAuthType,
  remotePassword,
  remotePrivateKeyPath,
  remoteCertificatePath,
  remoteDefaultPath,
  remoteHostKeyPolicy,
  remoteJumpEnabled,
  remoteJumpHost,
  remoteJumpPort,
  remoteJumpUsername,
  remoteJumpPrivateKeyPath,
  remoteJumpPassword,
  remoteError,
  remoteErrorAction,
  remoteSuccess,
  remoteTesting,
  remoteSaving,
  setRemoteModalOpen,
  setRemoteName,
  setRemoteHost,
  setRemotePort,
  setRemoteUsername,
  setRemoteAuthType,
  setRemotePassword,
  setRemotePrivateKeyPath,
  setRemoteCertificatePath,
  setRemoteDefaultPath,
  setRemoteHostKeyPolicy,
  setRemoteJumpEnabled,
  setRemoteJumpHost,
  setRemoteJumpPort,
  setRemoteJumpUsername,
  setRemoteJumpPrivateKeyPath,
  setRemoteJumpPassword,
  openKeyFilePicker,
  handleTestRemoteConnection,
  handleSaveRemoteConnection,
  keyFilePickerOpen,
  keyFilePickerTitle,
  keyFilePickerPath,
  keyFilePickerParent,
  keyFilePickerLoading,
  keyFilePickerItems,
  keyFilePickerError,
  closeKeyFilePicker,
  loadKeyFileEntries,
  applySelectedKeyFile,
  dirPickerOpen,
  dirPickerTarget,
  dirPickerPath,
  dirPickerParent,
  dirPickerLoading,
  dirPickerItems,
  dirPickerError,
  showHiddenDirs,
  dirPickerCreateModalOpen,
  dirPickerNewFolderName,
  dirPickerCreatingFolder,
  closeDirPicker,
  chooseDir,
  openCreateDirModal,
  setShowHiddenDirs,
  loadDirEntries,
  setDirPickerCreateModalOpen,
  setDirPickerNewFolderName,
  createDirInPicker,
  dialogState,
  handleConfirm,
  handleCancel,
}) => (
  <>
    <CreateContactModal
      isOpen={createContactModalOpen}
      agents={(agents || []) as any[]}
      existingAgentIds={existingContactAgentIds}
      selectedAgentId={selectedContactAgentId || ''}
      error={contactError}
      onClose={closeCreateSessionModal}
      onSelectedAgentChange={(agentId) => {
        setSelectedContactAgentId(agentId);
        setContactError(null);
      }}
      onCreate={() => {
        void handleCreateContactSession();
      }}
    />

    <CreateProjectModal
      isOpen={projectModalOpen}
      projectRoot={projectRoot}
      projectError={projectError}
      onClose={() => setProjectModalOpen(false)}
      onProjectRootChange={setProjectRoot}
      onOpenPicker={openDirPickerForProject}
      onCreate={() => {
        void handleCreateProject();
      }}
    />

    <CreateTerminalModal
      isOpen={terminalModalOpen}
      terminalRoot={terminalRoot}
      terminalError={terminalError}
      onClose={() => setTerminalModalOpen(false)}
      onTerminalRootChange={setTerminalRoot}
      onOpenPicker={openDirPickerForTerminal}
      onCreate={() => {
        void handleCreateTerminal();
      }}
    />

    <RemoteConnectionModal
      isOpen={remoteModalOpen}
      editingRemoteConnection={Boolean(editingRemoteConnectionId)}
      remoteName={remoteName}
      remoteHost={remoteHost}
      remotePort={remotePort}
      remoteUsername={remoteUsername}
      remoteAuthType={remoteAuthType}
      remotePassword={remotePassword}
      remotePrivateKeyPath={remotePrivateKeyPath}
      remoteCertificatePath={remoteCertificatePath}
      remoteDefaultPath={remoteDefaultPath}
      remoteHostKeyPolicy={remoteHostKeyPolicy}
      remoteJumpEnabled={remoteJumpEnabled}
      remoteJumpHost={remoteJumpHost}
      remoteJumpPort={remoteJumpPort}
      remoteJumpUsername={remoteJumpUsername}
      remoteJumpPrivateKeyPath={remoteJumpPrivateKeyPath}
      remoteJumpPassword={remoteJumpPassword}
      remoteError={remoteError}
      remoteErrorAction={remoteErrorAction}
      remoteSuccess={remoteSuccess}
      remoteTesting={remoteTesting}
      remoteSaving={remoteSaving}
      onClose={() => setRemoteModalOpen(false)}
      onRemoteNameChange={setRemoteName}
      onRemoteHostChange={setRemoteHost}
      onRemotePortChange={setRemotePort}
      onRemoteUsernameChange={setRemoteUsername}
      onRemoteAuthTypeChange={setRemoteAuthType}
      onRemotePasswordChange={setRemotePassword}
      onRemotePrivateKeyPathChange={setRemotePrivateKeyPath}
      onRemoteCertificatePathChange={setRemoteCertificatePath}
      onRemoteDefaultPathChange={setRemoteDefaultPath}
      onRemoteHostKeyPolicyChange={setRemoteHostKeyPolicy}
      onRemoteJumpEnabledChange={setRemoteJumpEnabled}
      onRemoteJumpHostChange={setRemoteJumpHost}
      onRemoteJumpPortChange={setRemoteJumpPort}
      onRemoteJumpUsernameChange={setRemoteJumpUsername}
      onRemoteJumpPrivateKeyPathChange={setRemoteJumpPrivateKeyPath}
      onRemoteJumpPasswordChange={setRemoteJumpPassword}
      onOpenKeyFilePicker={openKeyFilePicker}
      onTest={handleTestRemoteConnection}
      onSave={handleSaveRemoteConnection}
    />

    <KeyFilePickerDialog
      isOpen={keyFilePickerOpen}
      title={keyFilePickerTitle}
      currentPath={keyFilePickerPath || ''}
      parentPath={keyFilePickerParent}
      loading={keyFilePickerLoading}
      items={keyFilePickerItems}
      error={keyFilePickerError}
      onClose={closeKeyFilePicker}
      onBack={() => { void loadKeyFileEntries(keyFilePickerParent); }}
      onRefresh={() => { void loadKeyFileEntries(keyFilePickerPath); }}
      onEntryClick={(entry) => {
        if (entry.isDir) {
          void loadKeyFileEntries(entry.path);
        } else {
          applySelectedKeyFile(entry.path);
        }
      }}
      onSelectFile={applySelectedKeyFile}
    />

    <DirPickerDialog
      isOpen={dirPickerOpen}
      target={dirPickerTarget}
      currentPath={dirPickerPath || ''}
      parentPath={dirPickerParent}
      loading={dirPickerLoading}
      items={dirPickerItems}
      error={dirPickerError}
      showHiddenDirs={showHiddenDirs}
      createModalOpen={dirPickerCreateModalOpen}
      newFolderName={dirPickerNewFolderName}
      creatingFolder={dirPickerCreatingFolder}
      onClose={closeDirPicker}
      onBack={() => { void loadDirEntries(dirPickerParent); }}
      onChooseCurrent={() => chooseDir(dirPickerPath || '')}
      onOpenCreateModal={openCreateDirModal}
      onToggleHiddenDirs={() => setShowHiddenDirs((prev) => !prev)}
      onOpenEntry={(path) => { void loadDirEntries(path); }}
      onCreateModalClose={() => setDirPickerCreateModalOpen(false)}
      onNewFolderNameChange={setDirPickerNewFolderName}
      onCreateDir={() => { void createDirInPicker(); }}
    />

    <ConfirmDialog
      isOpen={dialogState.isOpen}
      title={dialogState.title}
      message={dialogState.message}
      description={dialogState.description}
      details={dialogState.details}
      detailsTitle={dialogState.detailsTitle}
      detailsLines={dialogState.detailsLines}
      confirmText={dialogState.confirmText}
      cancelText={dialogState.cancelText}
      type={dialogState.type}
      onConfirm={handleConfirm}
      onCancel={handleCancel}
    />
  </>
);
