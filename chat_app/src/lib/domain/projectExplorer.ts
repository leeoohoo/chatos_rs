// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  ProjectRunConfigFileSummaryResponse,
  ProjectRunCustomToolchainResponse,
  ProjectRunEnvironmentResponse,
  ProjectRunValidationIssueResponse,
  ProjectRunCatalogResponse,
  ProjectRunTargetResponse,
  ProjectRunToolchainOptionResponse,
} from '../api/client/types';
import type {
  ProjectRunConfigFileSummary,
  ProjectRunCustomToolchain,
  ProjectRunCatalog,
  ProjectRunEnvironment,
  ProjectRunState,
  ProjectRunTarget,
  ProjectRunToolchainOption,
  ProjectRunValidationIssue,
} from '../../types';
import {
  asRecord,
  readBooleanFirst,
  readFirst,
  readNumberFirst,
  readString,
  readStringFirst,
  readValue,
} from './normalizerUtils';
import { normalizeTerminal } from './terminals';

export const normalizeProjectRunTarget = (raw: ProjectRunTargetResponse | unknown): ProjectRunTarget => {
  const record = asRecord(raw);
  const command = readString(record, 'command');
  return {
    id: String(readValue(record, 'id') || ''),
    label: String(readValue(record, 'label') || command || ''),
    kind: String(readValue(record, 'kind') || 'custom'),
    language: readStringFirst(record, ['language']) || null,
    cwd: String(readValue(record, 'cwd') || ''),
    command,
    source: String(readValue(record, 'source') || 'auto'),
    confidence: readNumberFirst(record, ['confidence']),
    isDefault: readBooleanFirst(record, ['is_default', 'isDefault']),
    entrypoint: readStringFirst(record, ['entrypoint', 'entry_point']) || null,
    manifestPath: readStringFirst(record, ['manifest_path', 'manifestPath']) || null,
    requiredToolchains: Array.isArray(readFirst(record, ['required_toolchains', 'requiredToolchains']))
      ? (readFirst(record, ['required_toolchains', 'requiredToolchains']) as unknown[])
        .map((item) => String(item || '').trim())
        .filter(Boolean)
      : [],
  };
};

export const normalizeProjectRunCatalog = (raw: ProjectRunCatalogResponse | unknown): ProjectRunCatalog => {
  const record = asRecord(raw);
  const rawTargets = readValue(record, 'targets');
  const targets = Array.isArray(rawTargets)
    ? rawTargets.map(normalizeProjectRunTarget).filter((item: ProjectRunTarget) => item.id && item.command && item.cwd)
    : [];
  const defaultTargetId = readFirst(record, ['default_target_id', 'defaultTargetId']) ?? null;
  return {
    projectId: String(readFirst(record, ['project_id', 'projectId']) ?? ''),
    status: String(readValue(record, 'status') ?? (targets.length > 0 ? 'ready' : 'empty')),
    defaultTargetId: defaultTargetId ? String(defaultTargetId) : null,
    targets,
    errorMessage: (readFirst(record, ['error_message', 'errorMessage']) ?? null) as ProjectRunCatalog['errorMessage'],
    analyzedAt: (readFirst(record, ['analyzed_at', 'analyzedAt']) ?? null) as ProjectRunCatalog['analyzedAt'],
    updatedAt: (readFirst(record, ['updated_at', 'updatedAt']) ?? null) as ProjectRunCatalog['updatedAt'],
  };
};

export const normalizeProjectRunToolchainOption = (
  raw: ProjectRunToolchainOptionResponse | unknown,
): ProjectRunToolchainOption => {
  const record = asRecord(raw);
  return {
    id: String(readValue(record, 'id') || ''),
    kind: String(readValue(record, 'kind') || ''),
    label: String(readValue(record, 'label') || readValue(record, 'path') || ''),
    version: readStringFirst(record, ['version']) || null,
    path: String(readValue(record, 'path') || ''),
    source: String(readValue(record, 'source') || 'auto'),
    isDefault: readBooleanFirst(record, ['is_default', 'isDefault']),
  };
};

export const normalizeProjectRunCustomToolchain = (
  raw: ProjectRunCustomToolchainResponse | unknown,
): ProjectRunCustomToolchain => {
  const record = asRecord(raw);
  return {
    kind: String(readValue(record, 'kind') || ''),
    label: String(readValue(record, 'label') || ''),
    path: String(readValue(record, 'path') || ''),
  };
};

export const normalizeProjectRunConfigFileSummary = (
  raw: ProjectRunConfigFileSummaryResponse | unknown,
): ProjectRunConfigFileSummary => {
  const record = asRecord(raw);
  return {
    kind: String(readValue(record, 'kind') || ''),
    label: String(readValue(record, 'label') || ''),
    path: String(readValue(record, 'path') || ''),
    preview: readStringFirst(record, ['preview']) || null,
    source: String(readValue(record, 'source') || 'project-local'),
  };
};

export const normalizeProjectRunValidationIssue = (
  raw: ProjectRunValidationIssueResponse | unknown,
): ProjectRunValidationIssue => {
  const record = asRecord(raw);
  return {
    kind: String(readValue(record, 'kind') || ''),
    message: String(readValue(record, 'message') || ''),
    targetId: readStringFirst(record, ['target_id', 'targetId']) || null,
    targetLabel: readStringFirst(record, ['target_label', 'targetLabel']) || null,
    path: readStringFirst(record, ['path']) || null,
    hint: readStringFirst(record, ['hint']) || null,
  };
};

export const normalizeProjectRunEnvironment = (
  raw: ProjectRunEnvironmentResponse | unknown,
): ProjectRunEnvironment => {
  const record = asRecord(raw);
  const rawOptions = asRecord(readFirst(record, ['options_by_kind', 'optionsByKind'])) || {};
  const optionsByKind = Object.fromEntries(
    Object.entries(rawOptions).map(([kind, value]) => [
      kind,
      Array.isArray(value)
        ? value
          .map(normalizeProjectRunToolchainOption)
          .filter((item) => item.id && item.path)
        : [],
    ]),
  );

  const selectedToolchains = asRecord(readFirst(record, ['selected_toolchains', 'selectedToolchains'])) || {};
  const customToolchains = asRecord(readFirst(record, ['custom_toolchains', 'customToolchains'])) || {};
  const envVars = asRecord(readFirst(record, ['env_vars', 'envVars'])) || {};
  const rawConfigFiles = readFirst(record, ['config_files', 'configFiles']);
  const rawValidationIssues = readFirst(record, ['validation_issues', 'validationIssues']);

  return {
    projectId: String(readFirst(record, ['project_id', 'projectId']) ?? ''),
    userId: readStringFirst(record, ['user_id', 'userId']) || null,
    optionsByKind,
    configFiles: Array.isArray(rawConfigFiles)
      ? rawConfigFiles
        .map(normalizeProjectRunConfigFileSummary)
        .filter((item) => item.kind && item.path)
      : [],
    validationIssues: Array.isArray(rawValidationIssues)
      ? rawValidationIssues
        .map(normalizeProjectRunValidationIssue)
        .filter((item) => item.kind && item.message)
      : [],
    selectedToolchains: Object.fromEntries(
      Object.entries(selectedToolchains)
        .map(([key, value]) => [key, String(value || '').trim()])
        .filter(([, value]) => Boolean(value)),
    ),
    customToolchains: (() => {
      const out: Record<string, ProjectRunCustomToolchain> = {};
      Object.entries(customToolchains as Record<string, unknown>).forEach(([key, value]) => {
        const normalized = normalizeProjectRunCustomToolchain(value);
        const normalizedKind = normalized.kind.trim() || key.trim();
        if (!normalizedKind || !normalized.path.trim()) {
          return;
        }
        out[normalizedKind] = {
          ...normalized,
          kind: normalizedKind,
        };
      });
      return out;
    })(),
    envVars: Object.fromEntries(
      Object.entries(envVars)
        .map(([key, value]) => [key, String(value ?? '')]),
    ),
    terminalUiEnabled: readBooleanFirst(record, ['terminal_ui_enabled', 'terminalUiEnabled'], true),
    updatedAt: readStringFirst(record, ['updated_at', 'updatedAt']) || null,
  };
};

export const normalizeProjectRunState = (
  raw: import('../api/client/types').ProjectRunStateResponse | unknown,
): ProjectRunState => {
  const record = asRecord(raw);
  const terminalRaw = readValue(record, 'terminal');
  const terminal = terminalRaw ? normalizeTerminal(terminalRaw) : null;
  const rawInstances = readValue(record, 'instances');
  const instances = Array.isArray(rawInstances)
    ? rawInstances
      .map((item) => {
        const entry = asRecord(item);
        const terminalValue = readValue(entry, 'terminal');
        const normalizedTerminal = terminalValue ? normalizeTerminal(terminalValue) : null;
        const terminalId = readStringFirst(entry, ['terminal_id', 'terminalId']) || normalizedTerminal?.id || '';
        if (!terminalId) {
          return null;
        }
        return {
          terminalId,
          terminalName: readStringFirst(entry, ['terminal_name', 'terminalName']) || normalizedTerminal?.name || terminalId,
          cwd: readString(entry, 'cwd') || normalizedTerminal?.cwd || '',
          status: readString(entry, 'status') || normalizedTerminal?.status || 'idle',
          busy: readBooleanFirst(entry, ['busy']),
          running: readBooleanFirst(entry, ['running']),
          terminal: normalizedTerminal,
        };
      })
      .filter((item): item is NonNullable<typeof item> => Boolean(item))
    : [];
  return {
    projectId: readStringFirst(record, ['project_id', 'projectId']),
    running: readBooleanFirst(record, ['running']),
    busy: readBooleanFirst(record, ['busy']),
    status: readString(record, 'status') || 'idle',
    terminalId: readStringFirst(record, ['terminal_id', 'terminalId']) || terminal?.id || null,
    terminalName: readStringFirst(record, ['terminal_name', 'terminalName']) || terminal?.name || null,
    cwd: readString(record, 'cwd') || terminal?.cwd || null,
    terminal,
    instances,
  };
};
