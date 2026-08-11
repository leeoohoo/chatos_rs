// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

// @vitest-environment jsdom

import '@testing-library/jest-dom/vitest';
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { I18nProvider } from '../../../i18n/I18nProvider';
import type { FsEntry, Project } from '../../../types';
import { ProjectTreeHeader } from './ProjectTreeHeader';

afterEach(() => {
  cleanup();
});

const baseProjectRootEntry: FsEntry = {
  name: 'project',
  path: 'local://connector/device-1/workspace-1/project',
  isDir: true,
};

const createProject = (overrides: Partial<Project>): Project => ({
  id: 'project-1',
  name: 'Example project',
  rootPath: 'local://connector/device-1/workspace-1/project',
  createdAt: new Date('2026-08-11T00:00:00Z'),
  updatedAt: new Date('2026-08-11T00:00:00Z'),
  ...overrides,
});

const renderHeader = (project: Project) => {
  render(
    <I18nProvider>
      <ProjectTreeHeader
        project={project}
        projectRootEntry={baseProjectRootEntry}
        selectedEntry={null}
        draggingEntryPath={null}
        dropTargetDirPath={null}
        actionLoading={false}
        actionReloadPath={null}
        actionMessage={null}
        actionError={null}
        searchQuery=""
        searchCaseSensitive={false}
        searchWholeWord={false}
        searchResults={[]}
        totalSearchHits={0}
        canOpenPreviousSearchHit={false}
        canOpenNextSearchHit={false}
        activeSearchHitIndex={-1}
        searchLoading={false}
        searchError={null}
        searchTruncated={false}
        normalizePath={(value) => value}
        canDropToDirectory={() => false}
        onSelectProjectRoot={vi.fn()}
        onCreateDirectoryAtRoot={vi.fn()}
        onCreateFileAtRoot={vi.fn()}
        onRefresh={vi.fn()}
        onSearchQueryChange={vi.fn()}
        onToggleSearchCaseSensitive={vi.fn()}
        onToggleSearchWholeWord={vi.fn()}
        onClearSearch={vi.fn()}
        onOpenPreviousSearchHit={vi.fn()}
        onOpenNextSearchHit={vi.fn()}
        onOpenContextMenu={vi.fn()}
        onSetDropTargetDirPath={vi.fn()}
        onSetDraggingEntryPath={vi.fn()}
        onMoveEntryByDrop={vi.fn()}
        onClearDragExpandTimer={vi.fn()}
        onClearDragAutoScroll={vi.fn()}
      />
    </I18nProvider>,
  );
};

describe('ProjectTreeHeader', () => {
  it('shows the harness hint for cloud projects', () => {
    renderHeader(createProject({
      rootPath: '/workspace/cloud-project',
      sourceType: 'cloud',
    }));

    expect(
      screen.getByText('文件来自内部 Harness 仓库；编辑、创建和删除会直接提交到云端项目默认分支。'),
    ).toBeInTheDocument();
  });

  it('shows the gateway hint for local connector projects', () => {
    renderHeader(createProject({
      sourceType: 'local_connector',
    }));

    expect(
      screen.getByText('文件来自当前设备的 Local Connector 工作区；页面上的读写操作都会先经过云端网关，再转发到本机。'),
    ).toBeInTheDocument();
  });
});
