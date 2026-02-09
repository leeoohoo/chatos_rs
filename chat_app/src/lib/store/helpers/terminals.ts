import type { Terminal, TerminalLog } from '../../../types';

export const normalizeTerminal = (raw: any): Terminal => ({
  id: raw?.id,
  name: raw?.name ?? '',
  cwd: raw?.cwd ?? '',
  userId: raw?.user_id ?? raw?.userId ?? null,
  status: raw?.status ?? 'running',
  busy: raw?.busy ?? false,
  createdAt: new Date(raw?.created_at ?? raw?.createdAt ?? Date.now()),
  updatedAt: new Date(raw?.updated_at ?? raw?.updatedAt ?? raw?.created_at ?? raw?.createdAt ?? Date.now()),
  lastActiveAt: new Date(raw?.last_active_at ?? raw?.lastActiveAt ?? raw?.updated_at ?? raw?.updatedAt ?? Date.now()),
});

export const normalizeTerminalLog = (raw: any): TerminalLog => ({
  id: raw?.id,
  terminalId: raw?.terminal_id ?? raw?.terminalId ?? '',
  logType: raw?.log_type ?? raw?.logType ?? raw?.type ?? '',
  content: raw?.content ?? '',
  createdAt: raw?.created_at ?? raw?.createdAt ?? '',
});
