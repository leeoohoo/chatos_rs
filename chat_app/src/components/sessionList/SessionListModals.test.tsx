// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { CreateContactModal } from './CreateContactModal';
import { CreateProjectModal, CreateTerminalModal } from './CreateResourceModals';
import { DirPickerDialog } from './Pickers';
import { RemoteConnectionModal } from './RemoteConnectionModal';

describe('SessionList modals', () => {
  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
  });

  it('renders project and terminal creation dialogs through the shared manager dialog shell', () => {
    const onClose = vi.fn();
    const onCreate = vi.fn();

    const { rerender } = render(
      <CreateProjectModal
        isOpen
        projectRoot="/Users/demo/project-a"
        projectError={null}
        onClose={onClose}
        onProjectRootChange={vi.fn()}
        onOpenPicker={vi.fn()}
        onCreate={onCreate}
      />,
    );

    expect(screen.getByRole('dialog', { name: '新增项目' })).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '创建' }));
    expect(onCreate).toHaveBeenCalledTimes(1);

    rerender(
      <CreateTerminalModal
        isOpen
        terminalRoot="/Users/demo/project-a"
        terminalError={null}
        onClose={onClose}
        onTerminalRootChange={vi.fn()}
        onOpenPicker={vi.fn()}
        onCreate={onCreate}
      />,
    );

    expect(screen.getByRole('dialog', { name: '新增终端' })).toBeInTheDocument();
    expect(screen.getByDisplayValue('/Users/demo/project-a')).toBeInTheDocument();
  });

  it('layers the directory picker above the project creation dialog', () => {
    render(
      <>
        <CreateProjectModal
          isOpen
          projectRoot=""
          projectError={null}
          onClose={vi.fn()}
          onProjectRootChange={vi.fn()}
          onOpenPicker={vi.fn()}
          onCreate={vi.fn()}
        />
        <DirPickerDialog
          isOpen
          target="project"
          currentPath="/Users/demo"
          parentPath="/Users"
          writable
          loading={false}
          items={[]}
          error={null}
          showHiddenDirs={false}
          createModalOpen={false}
          newFolderName=""
          creatingFolder={false}
          onClose={vi.fn()}
          onBack={vi.fn()}
          onChooseCurrent={vi.fn()}
          onOpenCreateModal={vi.fn()}
          onToggleHiddenDirs={vi.fn()}
          onOpenEntry={vi.fn()}
          onCreateModalClose={vi.fn()}
          onNewFolderNameChange={vi.fn()}
          onCreateDir={vi.fn()}
        />
      </>,
    );

    const projectDialogLayer = screen.getByRole('dialog', { name: '新增项目' }).parentElement;
    const pickerLayer = screen.getByText('/Users/demo').closest('.fixed');

    expect(projectDialogLayer).toHaveClass('z-[70]');
    expect(pickerLayer).toHaveClass('z-[80]');
  });

  it('renders contact creation dialog with selectable agents', () => {
    const onCreate = vi.fn();
    const onSelectedAgentChange = vi.fn();

    render(
      <CreateContactModal
        isOpen
        agents={[
          { id: 'agent-1', name: '工程助理', description: '处理开发协作', enabled: true },
          { id: 'agent-2', name: '测试助理', enabled: false },
        ]}
        selectedAgentId="agent-1"
        error={null}
        onClose={vi.fn()}
        onSelectedAgentChange={onSelectedAgentChange}
        onCreate={onCreate}
      />,
    );

    expect(screen.getByRole('dialog', { name: '添加联系人' })).toBeInTheDocument();
    expect(screen.getByText('工程助理')).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /工程助理/ }));
    expect(onSelectedAgentChange).toHaveBeenCalledWith('agent-1');

    fireEvent.click(screen.getByRole('button', { name: '添加并开始聊天' }));
    expect(onCreate).toHaveBeenCalledTimes(1);
  });

  it('renders remote connection dialog with test and save actions', () => {
    const onTest = vi.fn();
    const onSave = vi.fn();

    render(
      <RemoteConnectionModal
        isOpen
        editingRemoteConnection={false}
        editingRemoteConnectionId={null}
        remoteConnections={[]}
        remoteName=""
        remoteHost="127.0.0.1"
        remotePort="22"
        remoteUsername="root"
        remoteAuthType="password"
        remotePassword=""
        remotePrivateKeyPath=""
        remoteCertificatePath=""
        remoteDefaultPath=""
        remoteHostKeyPolicy="strict"
        remoteJumpEnabled={false}
        remoteJumpMode="existing"
        remoteJumpConnectionId=""
        remoteJumpHost=""
        remoteJumpPort=""
        remoteJumpUsername=""
        remoteJumpPrivateKeyPath=""
        remoteJumpCertificatePath=""
        remoteJumpPassword=""
        remoteError={null}
        remoteErrorAction={null}
        remoteSuccess={null}
        remoteTesting={false}
        remoteSaving={false}
        remoteVerificationModalOpen={false}
        remoteVerificationPrompt=""
        remoteVerificationCode=""
        onClose={vi.fn()}
        onRemoteNameChange={vi.fn()}
        onRemoteHostChange={vi.fn()}
        onRemotePortChange={vi.fn()}
        onRemoteUsernameChange={vi.fn()}
        onRemoteAuthTypeChange={vi.fn()}
        onRemotePasswordChange={vi.fn()}
        onRemotePrivateKeyPathChange={vi.fn()}
        onRemoteCertificatePathChange={vi.fn()}
        onRemoteDefaultPathChange={vi.fn()}
        onRemoteHostKeyPolicyChange={vi.fn()}
        onRemoteJumpEnabledChange={vi.fn()}
        onRemoteJumpModeChange={vi.fn()}
        onRemoteJumpConnectionIdChange={vi.fn()}
        onRemoteJumpHostChange={vi.fn()}
        onRemoteJumpPortChange={vi.fn()}
        onRemoteJumpUsernameChange={vi.fn()}
        onRemoteJumpPrivateKeyPathChange={vi.fn()}
        onRemoteJumpCertificatePathChange={vi.fn()}
        onRemoteJumpPasswordChange={vi.fn()}
        onRemoteVerificationCodeChange={vi.fn()}
        onRemoteVerificationClose={vi.fn()}
        onRemoteVerificationSubmit={vi.fn()}
        onOpenKeyFilePicker={vi.fn()}
        onTest={onTest}
        onSave={onSave}
      />,
    );

    expect(screen.getByRole('dialog', { name: '新增远端连接' })).toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: '测试连接' }));
    expect(onTest).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole('button', { name: '创建' }));
    expect(onSave).toHaveBeenCalledTimes(1);
  });
});
