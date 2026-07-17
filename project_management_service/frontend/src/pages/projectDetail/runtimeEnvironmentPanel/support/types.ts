// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ProjectRuntimeEnvironmentVariableRecord } from '../../../../types';

export type JsonRecord = Record<string, unknown>;

export interface ServiceRow {
  rowKey: string;
  serviceKey: string;
  raw: unknown;
  record?: JsonRecord;
}

export interface EnvVarDraft extends ProjectRuntimeEnvironmentVariableRecord {
  rowKey: string;
  originalValue: string;
  draftValue: string;
  custom: boolean;
}

export interface LegacyEnvVarRow {
  key: string;
  name: string;
  value: unknown;
}
