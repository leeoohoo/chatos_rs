// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useMemo, useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Segmented, Space, Typography, message } from 'antd';

import { api } from '../api/client';
import { useI18n } from '../i18n/I18nProvider';
import {
  ToolingNoteDetailDrawer,
  ToolingTerminalLogDrawer,
} from './tooling/ToolingDrawers';
import {
  ToolingNotepadPanel,
  ToolingTerminalPanel,
} from './tooling/ToolingPanels';
import type { TerminalInputPayload } from './tooling/toolingPageUtils';

export function ToolingPage() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const [messageApi, contextHolder] = message.useMessage();
  const [view, setView] = useState<'notepad' | 'terminal'>('notepad');
  const [notepadUserId, setNotepadUserId] = useState('task_runner');
  const [folderFilter, setFolderFilter] = useState<string | undefined>(undefined);
  const [tagFilter, setTagFilter] = useState<string[]>([]);
  const [noteQueryText, setNoteQueryText] = useState('');
  const [selectedNoteId, setSelectedNoteId] = useState<string | null>(null);
  const [terminalUserId, setTerminalUserId] = useState('');
  const [terminalProjectId, setTerminalProjectId] = useState('');
  const [includeExited, setIncludeExited] = useState(true);
  const [selectedTerminalId, setSelectedTerminalId] = useState<string | null>(null);
  const [terminalInput, setTerminalInput] = useState('');

  const resolvedNotepadUserId = notepadUserId.trim() || 'task_runner';
  const resolvedTerminalUserId = terminalUserId.trim() || undefined;
  const resolvedTerminalProjectId = terminalProjectId.trim() || undefined;

  const notepadFoldersQuery = useQuery({
    queryKey: ['tooling', 'notepad', 'folders', resolvedNotepadUserId],
    queryFn: () => api.listToolingNotepadFolders(resolvedNotepadUserId),
  });
  const notepadTagsQuery = useQuery({
    queryKey: ['tooling', 'notepad', 'tags', resolvedNotepadUserId],
    queryFn: () => api.listToolingNotepadTags(resolvedNotepadUserId),
  });
  const notepadNotesQuery = useQuery({
    queryKey: [
      'tooling',
      'notepad',
      'notes',
      resolvedNotepadUserId,
      folderFilter,
      tagFilter.join(','),
      noteQueryText,
    ],
    queryFn: () =>
      api.listToolingNotepadNotes({
        userId: resolvedNotepadUserId,
        folder: folderFilter,
        tags: tagFilter,
        query: noteQueryText.trim() || undefined,
        limit: 200,
      }),
  });
  const selectedNoteQuery = useQuery({
    queryKey: ['tooling', 'notepad', 'note', resolvedNotepadUserId, selectedNoteId],
    queryFn: () => api.getToolingNotepadNote(selectedNoteId!, resolvedNotepadUserId),
    enabled: Boolean(selectedNoteId),
  });

  const terminalProcessesQuery = useQuery({
    queryKey: [
      'tooling',
      'terminal',
      'processes',
      resolvedTerminalUserId,
      resolvedTerminalProjectId,
      includeExited,
    ],
    queryFn: () =>
      api.listToolingTerminalProcesses({
        userId: resolvedTerminalUserId,
        projectId: resolvedTerminalProjectId,
        includeExited,
        limit: 100,
      }),
  });
  const selectedTerminalLogsQuery = useQuery({
    queryKey: [
      'tooling',
      'terminal',
      'logs',
      selectedTerminalId,
      resolvedTerminalUserId,
      resolvedTerminalProjectId,
    ],
    queryFn: () =>
      api.getToolingTerminalProcessLogs(selectedTerminalId!, {
        userId: resolvedTerminalUserId,
        projectId: resolvedTerminalProjectId,
        limit: 200,
      }),
    enabled: Boolean(selectedTerminalId),
  });

  const killTerminalMutation = useMutation({
    mutationFn: (terminalId: string) =>
      api.killToolingTerminalProcess(terminalId, {
        userId: resolvedTerminalUserId,
        projectId: resolvedTerminalProjectId,
      }),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ['tooling', 'terminal', 'processes'] }),
        queryClient.invalidateQueries({ queryKey: ['tooling', 'terminal', 'logs'] }),
      ]);
      messageApi.success(t('tooling.terminal.killed'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });
  const writeTerminalMutation = useMutation({
    mutationFn: ({ terminalId, data, submit }: TerminalInputPayload) =>
      api.writeToolingTerminalProcess(terminalId, {
        userId: resolvedTerminalUserId,
        projectId: resolvedTerminalProjectId,
        data,
        submit,
      }),
    onSuccess: async () => {
      setTerminalInput('');
      await queryClient.invalidateQueries({ queryKey: ['tooling', 'terminal', 'logs'] });
      messageApi.success(t('tooling.terminal.inputSent'));
    },
    onError: (error: Error) => messageApi.error(error.message),
  });

  const selectedTerminal = useMemo(
    () =>
      (terminalProcessesQuery.data?.processes || []).find(
        (process) => process.terminal_id === selectedTerminalId,
      ) || null,
    [selectedTerminalId, terminalProcessesQuery.data?.processes],
  );

  const refreshNotepad = () => {
    void notepadFoldersQuery.refetch();
    void notepadTagsQuery.refetch();
    void notepadNotesQuery.refetch();
  };

  const clearNotepadFilters = () => {
    setFolderFilter(undefined);
    setTagFilter([]);
    setNoteQueryText('');
  };

  return (
    <>
      {contextHolder}
      <Space direction="vertical" size="large" style={{ width: '100%' }}>
        <Space style={{ justifyContent: 'space-between', width: '100%' }}>
          <Space direction="vertical" size={0}>
            <Typography.Title level={3} style={{ margin: 0 }}>
              {t('tooling.title')}
            </Typography.Title>
            <Typography.Text type="secondary">{t('tooling.subtitle')}</Typography.Text>
          </Space>
          <Segmented
            value={view}
            onChange={(value) => setView(value as 'notepad' | 'terminal')}
            options={[
              { label: 'Notepad', value: 'notepad' },
              { label: 'Terminal', value: 'terminal' },
            ]}
          />
        </Space>

        {view === 'notepad' ? (
          <ToolingNotepadPanel
            t={t}
            userId={notepadUserId}
            folderFilter={folderFilter}
            tagFilter={tagFilter}
            queryText={noteQueryText}
            folders={notepadFoldersQuery.data?.folders || []}
            tags={notepadTagsQuery.data?.tags || []}
            notes={notepadNotesQuery.data?.notes || []}
            loading={notepadNotesQuery.isLoading}
            onUserIdChange={setNotepadUserId}
            onFolderFilterChange={setFolderFilter}
            onTagFilterChange={setTagFilter}
            onQueryTextChange={setNoteQueryText}
            onClearFilters={clearNotepadFilters}
            onRefresh={refreshNotepad}
            onOpenNote={setSelectedNoteId}
          />
        ) : (
          <ToolingTerminalPanel
            t={t}
            userId={terminalUserId}
            projectId={terminalProjectId}
            includeExited={includeExited}
            processes={terminalProcessesQuery.data?.processes || []}
            loading={terminalProcessesQuery.isLoading}
            killLoading={killTerminalMutation.isPending}
            onUserIdChange={setTerminalUserId}
            onProjectIdChange={setTerminalProjectId}
            onIncludeExitedChange={setIncludeExited}
            onRefresh={() => {
              void terminalProcessesQuery.refetch();
            }}
            onOpenLogs={setSelectedTerminalId}
            onKill={killTerminalMutation.mutate}
          />
        )}
      </Space>

      <ToolingNoteDetailDrawer
        t={t}
        open={Boolean(selectedNoteId)}
        data={selectedNoteQuery.data}
        loading={selectedNoteQuery.isLoading}
        onClose={() => setSelectedNoteId(null)}
      />

      <ToolingTerminalLogDrawer
        t={t}
        open={Boolean(selectedTerminalId)}
        title={selectedTerminal?.terminal_name || t('tooling.terminal.logTitle')}
        data={selectedTerminalLogsQuery.data}
        loading={selectedTerminalLogsQuery.isLoading}
        input={terminalInput}
        killLoading={killTerminalMutation.isPending}
        writeLoading={writeTerminalMutation.isPending}
        onClose={() => {
          setSelectedTerminalId(null);
          setTerminalInput('');
        }}
        onRefresh={() => {
          void selectedTerminalLogsQuery.refetch();
        }}
        onInputChange={setTerminalInput}
        onKill={killTerminalMutation.mutate}
        onSendInput={writeTerminalMutation.mutate}
      />
    </>
  );
}
