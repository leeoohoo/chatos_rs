// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import type { AgentConfig, FsEntry } from '../../types';
import type { TaskRunnerAgentAccountResponse } from '../../lib/api/client/types';
import { CreateContactModal } from './CreateContactModal';
import {
  CreateProjectModal,
  CreateTerminalModal,
  type LocalConnectorDirectoryEntryOption,
  type LocalConnectorWorkspaceOption,
  type ResourceSourceMode,
} from './CreateResourceModals';
import { DirPickerDialog, KeyFilePickerDialog } from './Pickers';
import { RemoteConnectionModal } from './RemoteConnectionModal';
import { TaskRunnerConfigModal } from './TaskRunnerConfigModal';
import type { RemoteConnection } from '../../types';
import type { ContactItem } from './types';
import type {
  HostKeyPolicy,
  JumpHostMode,
  KeyFilePickerTarget,
  RemoteAuthType,
} from './helpers';

interface SessionListDialogsProps {
  createContactModalOpen: boolean;
  agents: AgentConfig[];
  existingContactAgentIds: string[];
  selectedContactAgentId: string | null;
  contactError: string | null;
  closeCreateSessionModal: () => void;
  setSelectedContactAgentId: (value: string) => void;
  setContactError: (value: string | null) => void;
  handleCreateContactSession: () => Promise<void> | void;
  taskRunnerContact: ContactItem | null;
  taskRunnerAgentAccounts: TaskRunnerAgentAccountResponse[];
  taskRunnerAgentAccountsLoading: boolean;
  taskRunnerError: string | null;
  taskRunnerSaving: boolean;
  closeTaskRunnerConfig: () => void;
  saveTaskRunnerConfig: (values: {
    enabled: boolean;
    agentAccountId: string;
  }) => Promise<void> | void;

  projectModalOpen: boolean;
  projectRoot: string;
  cloudProjectName: string;
  cloudProjectGitUrl: string;
  cloudProjectZipFile: File | null;
  projectError: string | null;
  projectSourceMode: ResourceSourceMode;
  allowProjectCreation: boolean;
  localConnectorWorkspaces: LocalConnectorWorkspaceOption[];
  localConnectorLoading: boolean;
  localConnectorError: string | null;
  localConnectorDirectoryPath: string;
  localConnectorDirectoryParent: string | null;
  localConnectorDirectoryEntries: LocalConnectorDirectoryEntryOption[];
  localConnectorDirectoryLoading: boolean;
  localConnectorDirectoryError: string | null;
  selectedLocalConnectorDirectoryPath: string;
  selectedLocalConnectorWorkspaceId: string;
  setProjectModalOpen: (value: boolean) => void;
  setProjectSourceMode: (value: ResourceSourceMode) => void;
  setProjectRoot: (value: string) => void;
  setCloudProjectName: (value: string) => void;
  setCloudProjectGitUrl: (value: string) => void;
  setCloudProjectZipFile: (value: File | null) => void;
  openDirPickerForProject: () => void;
  refreshLocalConnectorWorkspaces: () => Promise<void> | void;
  setSelectedLocalConnectorWorkspaceId: (value: string) => void;
  browseLocalConnectorDirectory: (path: string) => void;
  setSelectedLocalConnectorDirectoryPath: (value: string) => void;
  createLocalConnectorDirectory: (name: string) => Promise<void> | void;
  handleCreateProject: () => Promise<void> | void;

  terminalModalOpen: boolean;
  terminalError: string | null;
  terminalExecuting: boolean;
  setTerminalModalOpen: (value: boolean) => void;
  handleCreateTerminal: () => Promise<void> | void;

  remoteModalOpen: boolean;
  editingRemoteConnectionId: string | null;
  remoteConnections: RemoteConnection[];
  remoteName: string;
  remoteHost: string;
  remotePort: string;
  remoteUsername: string;
  remoteAuthType: RemoteAuthType;
  remotePassword: string;
  remotePrivateKeyPath: string;
  remoteCertificatePath: string;
  remoteDefaultPath: string;
  remoteHostKeyPolicy: HostKeyPolicy;
  remoteJumpEnabled: boolean;
  remoteJumpMode: JumpHostMode;
  remoteJumpConnectionId: string;
  remoteJumpHost: string;
  remoteJumpPort: string;
  remoteJumpUsername: string;
  remoteJumpPrivateKeyPath: string;
  remoteJumpCertificatePath: string;
  remoteJumpPassword: string;
  remoteError: string | null;
  remoteErrorAction: string | null;
  remoteSuccess: string | null;
  remoteTesting: boolean;
  remoteSaving: boolean;
  remoteVerificationModalOpen: boolean;
  remoteVerificationPrompt: string;
  remoteVerificationCode: string;
  setRemoteModalOpen: (value: boolean) => void;
  setRemoteName: (value: string) => void;
  setRemoteHost: (value: string) => void;
  setRemotePort: (value: string) => void;
  setRemoteUsername: (value: string) => void;
  setRemoteAuthType: (value: RemoteAuthType) => void;
  setRemotePassword: (value: string) => void;
  setRemotePrivateKeyPath: (value: string) => void;
  setRemoteCertificatePath: (value: string) => void;
  setRemoteDefaultPath: (value: string) => void;
  setRemoteHostKeyPolicy: (value: HostKeyPolicy) => void;
  setRemoteJumpEnabled: (value: boolean) => void;
  setRemoteJumpMode: (value: JumpHostMode) => void;
  setRemoteJumpConnectionId: (value: string) => void;
  setRemoteJumpHost: (value: string) => void;
  setRemoteJumpPort: (value: string) => void;
  setRemoteJumpUsername: (value: string) => void;
  setRemoteJumpPrivateKeyPath: (value: string) => void;
  setRemoteJumpCertificatePath: (value: string) => void;
  setRemoteJumpPassword: (value: string) => void;
  setRemoteVerificationCode: (value: string) => void;
  setRemoteVerificationModalOpen: (value: boolean) => void;
  openKeyFilePicker: (target: KeyFilePickerTarget) => void;
  handleTestRemoteConnection: () => Promise<void> | void;
  handleSaveRemoteConnection: () => Promise<void> | void;
  handleSubmitRemoteVerification: () => Promise<void> | void;

  keyFilePickerOpen: boolean;
  keyFilePickerTitle: string;
  keyFilePickerPath: string | null;
  keyFilePickerParent: string | null;
  keyFilePickerLoading: boolean;
  keyFilePickerItems: FsEntry[];
  keyFilePickerError: string | null;
  closeKeyFilePicker: () => void;
  loadKeyFileEntries: (path: string | null) => Promise<void> | void;
  applySelectedKeyFile: (path: string) => void;

  dirPickerOpen: boolean;
  dirPickerPath: string | null;
  dirPickerParent: string | null;
  dirPickerWritable: boolean;
  dirPickerLoading: boolean;
  dirPickerItems: FsEntry[];
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
  taskRunnerContact,
  taskRunnerAgentAccounts,
  taskRunnerAgentAccountsLoading,
  taskRunnerError,
  taskRunnerSaving,
  closeTaskRunnerConfig,
  saveTaskRunnerConfig,
  projectModalOpen,
  projectRoot,
  cloudProjectName,
  cloudProjectGitUrl,
  cloudProjectZipFile,
  projectError,
  projectSourceMode,
  allowProjectCreation,
  localConnectorWorkspaces,
  localConnectorLoading,
  localConnectorError,
  localConnectorDirectoryPath,
  localConnectorDirectoryParent,
  localConnectorDirectoryEntries,
  localConnectorDirectoryLoading,
  localConnectorDirectoryError,
  selectedLocalConnectorDirectoryPath,
  selectedLocalConnectorWorkspaceId,
  setProjectModalOpen,
  setProjectSourceMode,
  setProjectRoot,
  setCloudProjectName,
  setCloudProjectGitUrl,
  setCloudProjectZipFile,
  openDirPickerForProject,
  refreshLocalConnectorWorkspaces,
  setSelectedLocalConnectorWorkspaceId,
  browseLocalConnectorDirectory,
  setSelectedLocalConnectorDirectoryPath,
  createLocalConnectorDirectory,
  handleCreateProject,
  terminalModalOpen,
  terminalError,
  terminalExecuting,
  setTerminalModalOpen,
  handleCreateTerminal,
  remoteModalOpen,
  editingRemoteConnectionId,
  remoteConnections,
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
  remoteJumpMode,
  remoteJumpConnectionId,
  remoteJumpHost,
  remoteJumpPort,
  remoteJumpUsername,
  remoteJumpPrivateKeyPath,
  remoteJumpCertificatePath,
  remoteJumpPassword,
  remoteError,
  remoteErrorAction,
  remoteSuccess,
  remoteTesting,
  remoteSaving,
  remoteVerificationModalOpen,
  remoteVerificationPrompt,
  remoteVerificationCode,
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
  setRemoteJumpMode,
  setRemoteJumpConnectionId,
  setRemoteJumpHost,
  setRemoteJumpPort,
  setRemoteJumpUsername,
  setRemoteJumpPrivateKeyPath,
  setRemoteJumpCertificatePath,
  setRemoteJumpPassword,
  setRemoteVerificationCode,
  setRemoteVerificationModalOpen,
  openKeyFilePicker,
  handleTestRemoteConnection,
  handleSaveRemoteConnection,
  handleSubmitRemoteVerification,
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
  dirPickerPath,
  dirPickerParent,
  dirPickerWritable,
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
}) => {
  const [projectCreating, setProjectCreating] = React.useState(false);

  React.useEffect(() => {
    if (!projectModalOpen) {
      setProjectCreating(false);
    }
  }, [projectModalOpen]);

  const closeProjectModal = React.useCallback(() => {
    if (!projectCreating) {
      setProjectModalOpen(false);
    }
  }, [projectCreating, setProjectModalOpen]);

  const submitProject = React.useCallback(async () => {
    if (projectCreating) {
      return;
    }
    setProjectCreating(true);
    try {
      await handleCreateProject();
    } finally {
      setProjectCreating(false);
    }
  }, [handleCreateProject, projectCreating]);

  return (
  <>
    <CreateContactModal
      isOpen={createContactModalOpen}
      agents={agents || []}
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

    <TaskRunnerConfigModal
      isOpen={Boolean(taskRunnerContact)}
      contact={taskRunnerContact}
      agentAccounts={taskRunnerAgentAccounts}
      loadingAgentAccounts={taskRunnerAgentAccountsLoading}
      saving={taskRunnerSaving}
      error={taskRunnerError}
      onClose={closeTaskRunnerConfig}
      onSave={saveTaskRunnerConfig}
    />

    <CreateProjectModal
      isOpen={projectModalOpen}
      projectRoot={projectRoot}
      cloudProjectName={cloudProjectName}
      cloudProjectGitUrl={cloudProjectGitUrl}
      cloudProjectZipFile={cloudProjectZipFile}
      projectError={projectError}
      sourceMode={projectSourceMode}
      allowLocalConnector={allowProjectCreation}
      localConnectorWorkspaces={localConnectorWorkspaces}
      localConnectorLoading={localConnectorLoading}
      localConnectorError={localConnectorError}
      localConnectorDirectoryPath={localConnectorDirectoryPath}
      localConnectorDirectoryParent={localConnectorDirectoryParent}
      localConnectorDirectoryEntries={localConnectorDirectoryEntries}
      localConnectorDirectoryLoading={localConnectorDirectoryLoading}
      localConnectorDirectoryError={localConnectorDirectoryError}
      selectedLocalDirectoryPath={selectedLocalConnectorDirectoryPath}
      selectedLocalWorkspaceId={selectedLocalConnectorWorkspaceId}
      submitting={projectCreating}
      onClose={closeProjectModal}
      onSourceModeChange={setProjectSourceMode}
      onProjectRootChange={setProjectRoot}
      onCloudProjectNameChange={setCloudProjectName}
      onCloudProjectGitUrlChange={setCloudProjectGitUrl}
      onCloudProjectZipFileChange={setCloudProjectZipFile}
      onOpenPicker={openDirPickerForProject}
      onRefreshLocalConnector={refreshLocalConnectorWorkspaces}
      onSelectedLocalWorkspaceChange={setSelectedLocalConnectorWorkspaceId}
      onBrowseLocalConnectorDirectory={browseLocalConnectorDirectory}
      onSelectLocalConnectorDirectory={setSelectedLocalConnectorDirectoryPath}
      onCreateLocalConnectorDirectory={createLocalConnectorDirectory}
      onCreate={() => {
        void submitProject();
      }}
    />

    <CreateTerminalModal
      isOpen={terminalModalOpen}
      terminalError={terminalError}
      localConnectorWorkspaces={localConnectorWorkspaces}
      localConnectorLoading={localConnectorLoading}
      localConnectorError={localConnectorError}
      localConnectorDirectoryPath={localConnectorDirectoryPath}
      localConnectorDirectoryParent={localConnectorDirectoryParent}
      localConnectorDirectoryEntries={localConnectorDirectoryEntries}
      localConnectorDirectoryLoading={localConnectorDirectoryLoading}
      localConnectorDirectoryError={localConnectorDirectoryError}
      selectedLocalDirectoryPath={selectedLocalConnectorDirectoryPath}
      selectedLocalWorkspaceId={selectedLocalConnectorWorkspaceId}
      terminalExecuting={terminalExecuting}
      onClose={() => setTerminalModalOpen(false)}
      onRefreshLocalConnector={refreshLocalConnectorWorkspaces}
      onSelectedLocalWorkspaceChange={setSelectedLocalConnectorWorkspaceId}
      onBrowseLocalConnectorDirectory={browseLocalConnectorDirectory}
      onSelectLocalConnectorDirectory={setSelectedLocalConnectorDirectoryPath}
      onCreateLocalConnectorDirectory={createLocalConnectorDirectory}
      onCreate={() => {
        void handleCreateTerminal();
      }}
    />

    <RemoteConnectionModal
      isOpen={remoteModalOpen}
      editingRemoteConnection={Boolean(editingRemoteConnectionId)}
      editingRemoteConnectionId={editingRemoteConnectionId}
      remoteConnections={remoteConnections}
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
      remoteJumpMode={remoteJumpMode}
      remoteJumpConnectionId={remoteJumpConnectionId}
      remoteJumpHost={remoteJumpHost}
      remoteJumpPort={remoteJumpPort}
      remoteJumpUsername={remoteJumpUsername}
      remoteJumpPrivateKeyPath={remoteJumpPrivateKeyPath}
      remoteJumpCertificatePath={remoteJumpCertificatePath}
      remoteJumpPassword={remoteJumpPassword}
      remoteError={remoteError}
      remoteErrorAction={remoteErrorAction}
      remoteSuccess={remoteSuccess}
      remoteTesting={remoteTesting}
      remoteSaving={remoteSaving}
      remoteVerificationModalOpen={remoteVerificationModalOpen}
      remoteVerificationPrompt={remoteVerificationPrompt}
      remoteVerificationCode={remoteVerificationCode}
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
      onRemoteJumpModeChange={setRemoteJumpMode}
      onRemoteJumpConnectionIdChange={setRemoteJumpConnectionId}
      onRemoteJumpHostChange={setRemoteJumpHost}
      onRemoteJumpPortChange={setRemoteJumpPort}
      onRemoteJumpUsernameChange={setRemoteJumpUsername}
      onRemoteJumpPrivateKeyPathChange={setRemoteJumpPrivateKeyPath}
      onRemoteJumpCertificatePathChange={setRemoteJumpCertificatePath}
      onRemoteJumpPasswordChange={setRemoteJumpPassword}
      onRemoteVerificationCodeChange={setRemoteVerificationCode}
      onRemoteVerificationClose={() => setRemoteVerificationModalOpen(false)}
      onRemoteVerificationSubmit={handleSubmitRemoteVerification}
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
      currentPath={dirPickerPath || ''}
      parentPath={dirPickerParent}
      writable={dirPickerWritable}
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
  </>
  );
};
