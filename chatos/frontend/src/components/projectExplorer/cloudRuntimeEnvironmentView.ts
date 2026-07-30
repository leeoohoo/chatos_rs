// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ProjectRuntimeEnvironmentRecordResponse,
  ProjectRuntimeEnvironmentResponse,
} from '../../lib/api/client/types';

type UnknownRecord = Record<string, unknown>;

export const asRecord = (value: unknown): UnknownRecord => (
  value && typeof value === 'object' && !Array.isArray(value)
    ? value as UnknownRecord
    : {}
);

export const readString = (record: UnknownRecord, keys: string[], fallback = ''): string => {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) {
      return value.trim();
    }
  }
  return fallback;
};

export const readBoolean = (record: UnknownRecord, keys: string[], fallback = false): boolean => {
  for (const key of keys) {
    if (typeof record[key] === 'boolean') {
      return record[key] as boolean;
    }
  }
  return fallback;
};

export const readNumber = (record: UnknownRecord, keys: string[]): number | null => {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'number' && Number.isFinite(value)) {
      return value;
    }
  }
  return null;
};

export const environmentRecord = (
  response: ProjectRuntimeEnvironmentResponse | null,
): ProjectRuntimeEnvironmentRecordResponse => response?.environment || {};

const formatJson = (value: unknown): string => {
  if (value == null) {
    return '';
  }
  if (typeof value === 'string') {
    return value;
  }
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
};

export const formatDateTime = (value: string): string => {
  if (!value) {
    return '-';
  }
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
};

export const formatElapsed = (startedAt: string): string => {
  if (!startedAt) {
    return '-';
  }
  const started = new Date(startedAt).getTime();
  if (!Number.isFinite(started)) {
    return '-';
  }
  const totalSeconds = Math.max(0, Math.floor((Date.now() - started) / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return hours > 0
    ? `${hours}h ${minutes}m ${seconds}s`
    : `${minutes}m ${seconds}s`;
};

export const statusTone = (status: string): string => {
  if (status === 'ready') {
    return 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700';
  }
  if (status === 'failed' || status === 'not_runnable') {
    return 'border-destructive/30 bg-destructive/10 text-destructive';
  }
  if (status === 'analyzing' || status === 'pending' || status === 'pending_image_build') {
    return 'border-amber-500/30 bg-amber-500/10 text-amber-700';
  }
  return 'border-border bg-background text-muted-foreground';
};

export const displayValue = (value: unknown): string => {
  if (value == null || value === '') {
    return '-';
  }
  if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
    return String(value);
  }
  return formatJson(value);
};

const readStringArray = (record: UnknownRecord, keys: string[]): string[] => {
  for (const key of keys) {
    const value = record[key];
    if (Array.isArray(value)) {
      return value
        .map((item) => {
          if (typeof item === 'string' || typeof item === 'number') {
            return String(item).trim();
          }
          const itemRecord = asRecord(item);
          return readString(itemRecord, ['name', 'path', 'file', 'id', 'type']);
        })
        .filter(Boolean);
    }
  }
  return [];
};

const uniqueValues = (values: string[]): string[] => Array.from(new Set(
  values.map((value) => value.trim()).filter(Boolean),
));

const stackHasContent = (value: unknown): boolean => {
  if (value == null) {
    return false;
  }
  if (typeof value === 'string') {
    return Boolean(value.trim());
  }
  if (Array.isArray(value)) {
    return value.length > 0;
  }
  if (typeof value === 'object') {
    return Object.keys(value).length > 0;
  }
  return true;
};

interface DetectedStackView {
  hasContent: boolean;
  summaryItems: string[];
  groups: Array<{ label: string; items: string[] }>;
  files: string[];
  metrics: Array<{ label: string; value: string }>;
}

const splitSummaryItems = (summary: string): string[] => {
  const paragraphs = summary
    .split(/\r?\n+/)
    .map((item) => item.trim())
    .filter(Boolean);
  const items: string[] = [];
  paragraphs.forEach((paragraph) => {
    let buffer = '';
    for (let index = 0; index < paragraph.length; index += 1) {
      const char = paragraph[index];
      const previous = paragraph[index - 1] ?? '';
      const next = paragraph[index + 1] ?? '';
      buffer += char;
      const isBoundary = ['。', '！', '？', '；', ';', '!', '?'].includes(char)
        || (char === '.' && !/\d/.test(previous) && /\s/.test(next));
      if (isBoundary) {
        const text = buffer.trim();
        if (text) {
          items.push(text);
        }
        buffer = '';
        while (/\s/.test(paragraph[index + 1] ?? '')) {
          index += 1;
        }
      }
    }
    const rest = buffer.trim();
    if (rest) {
      items.push(rest);
    }
  });
  return items.length > 0 ? items : summary.trim() ? [summary.trim()] : [];
};

export const buildDetectedStackView = (
  value: unknown,
  t: (key: string) => string,
): DetectedStackView => {
  if (!stackHasContent(value)) {
    return { hasContent: false, summaryItems: [], groups: [], files: [], metrics: [] };
  }
  if (typeof value === 'string') {
    return { hasContent: true, summaryItems: splitSummaryItems(value.trim()), groups: [], files: [], metrics: [] };
  }
  const record = asRecord(value);
  const groupSpecs: Array<[string, string[]]> = [
    [t('cloudRuntime.stack.languages'), ['languages', 'language']],
    [t('cloudRuntime.stack.frameworks'), ['frameworks', 'framework']],
    [t('cloudRuntime.stack.runtimes'), ['runtimes', 'runtime', 'runtime_versions', 'runtimeVersions']],
    [t('cloudRuntime.stack.packageManagers'), ['package_managers', 'packageManagers', 'package_manager', 'packageManager']],
    [t('cloudRuntime.stack.databases'), ['databases', 'database', 'data_stores', 'dataStores']],
    [t('cloudRuntime.stack.manifests'), ['manifests', 'manifest_files', 'manifestFiles']],
    [t('cloudRuntime.stack.entrypoints'), ['entrypoints', 'entry_points', 'entryPoints', 'start_commands', 'startCommands']],
  ];
  const consumed = new Set<string>();
  const groups = groupSpecs
    .map(([label, keys]) => {
      keys.forEach((key) => consumed.add(key));
      return { label, items: uniqueValues(readStringArray(record, keys)) };
    })
    .filter((group) => group.items.length > 0);
  const fileKeys = [
    'source_files',
    'sourceFiles',
    'files',
    'matched_files',
    'matchedFiles',
    'reference_files',
    'referenceFiles',
    'evidence_files',
    'evidenceFiles',
  ];
  fileKeys.forEach((key) => consumed.add(key));
  const files = uniqueValues(readStringArray(record, fileKeys));
  const summary = readString(record, ['summary', 'analysis_summary', 'analysisSummary', 'description', 'detail']);
  ['summary', 'analysis_summary', 'analysisSummary', 'description', 'detail', 'source'].forEach((key) => consumed.add(key));
  const otherSignals = Object.entries(record)
    .filter(([key, item]) => !consumed.has(key) && Array.isArray(item))
    .flatMap(([key, item]) => readStringArray({ [key]: item }, [key]).slice(0, 6));
  if (otherSignals.length > 0) {
    groups.push({
      label: t('cloudRuntime.stack.otherSignals'),
      items: uniqueValues(otherSignals).slice(0, 10),
    });
  }
  const metrics = [
    ['reference_count', 'referenceCount', t('cloudRuntime.stack.referenceCount')],
    ['scanned_file_count', 'scannedFileCount', 'file_count', 'fileCount', t('cloudRuntime.stack.scannedFileCount')],
  ]
    .map((keys) => {
      const label = keys[keys.length - 1];
      const metricKeys = keys.slice(0, -1);
      const value = metricKeys
        .map((key) => record[key])
        .find((item) => typeof item === 'number' || typeof item === 'string');
      return value == null ? undefined : { label, value: String(value) };
    })
    .filter((item): item is { label: string; value: string } => Boolean(item));
  return {
    hasContent: Boolean(summary || groups.length > 0 || files.length > 0 || metrics.length > 0),
    summaryItems: splitSummaryItems(summary),
    groups,
    files,
    metrics,
  };
};

export const serviceConfigEntries = (
  service: UnknownRecord,
  t: (key: string) => string,
): Array<{ label: string; value: string }> => {
  const config = asRecord(service.config ?? service.configuration ?? service);
  const entries: Array<{ label: string; value: string }> = [];
  const push = (label: string, value: unknown) => {
    if (value == null || value === '') {
      return;
    }
    if (typeof value === 'boolean') {
      entries.push({ label, value: value ? t('cloudRuntime.yes') : t('cloudRuntime.no') });
      return;
    }
    if (typeof value === 'string' || typeof value === 'number') {
      const text = String(value).trim();
      if (text) {
        entries.push({ label, value: text });
      }
    }
  };
  push(t('cloudRuntime.serviceVersion'), config.version);
  push(t('cloudRuntime.serviceDatabase'), config.database ?? config.db);
  push(t('cloudRuntime.serviceUsername'), config.username ?? config.user);
  push(t('cloudRuntime.serviceRequired'), config.required);
  const ports = formatServicePorts(config.ports ?? service.ports);
  if (ports) {
    entries.push({ label: t('cloudRuntime.ports'), value: ports });
  }
  push(t('cloudRuntime.environment'), config.environment_key ?? config.environmentKey);
  return entries;
};

const formatServicePorts = (value: unknown): string => {
  if (!Array.isArray(value)) {
    return '';
  }
  return value
    .map((port) => {
      const record = asRecord(port);
      const containerPort = record.container_port ?? record.containerPort ?? record.port;
      const protocol = readString(record, ['protocol']);
      if (typeof containerPort === 'number' || typeof containerPort === 'string') {
        return `${containerPort}${protocol ? `/${protocol}` : ''}`;
      }
      return '';
    })
    .filter(Boolean)
    .join(', ');
};
