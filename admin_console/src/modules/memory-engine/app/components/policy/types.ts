// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { EngineJobPolicy } from '../../../types';
import type {
  PolicyMeta,
} from '../../types';

export type PolicyEditorCardProps = {
  policy: EngineJobPolicy;
  meta: PolicyMeta;
};

export type PolicySummaryProps = Pick<PolicyEditorCardProps, 'meta'> & {
  updatedAt: string;
};

export type PolicyFieldsProps = {
  policy: EngineJobPolicy;
  meta: PolicyMeta;
};
