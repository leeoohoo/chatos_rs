// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  EngineSource,
} from '../../types';
import type {
  SourceFormValues,
  SourcePayloadResult,
} from '../types';

import { textOrUndefined } from './common';

export function buildSourcePayload(values: SourceFormValues): SourcePayloadResult {
  return {
    sourceId: values.source_id.trim(),
    payload: {
      tenant_id: textOrUndefined(values.tenant_id) ?? null,
      source_type: 'sdk_system',
      name: values.name.trim(),
      description: textOrUndefined(values.description) ?? null,
      config: null,
      sdk_enabled: true,
      status: values.enabled ? 'active' : 'disabled',
    },
  };
}

export function sourceFormInitialValues(source?: EngineSource | null): SourceFormValues {
  return {
    tenant_id: source?.tenant_id ?? '',
    source_id: source?.source_id ?? '',
    name: source?.name ?? '',
    description: source?.description ?? '',
    enabled: (source?.status ?? 'active') === 'active',
  };
}
