// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { Card } from 'antd';

import { PolicyFields } from './policy/PolicyFields';
import { PolicySummary } from './policy/PolicySummary';
import type { PolicyEditorCardProps } from './policy/types';

export function PolicyEditorCard(props: PolicyEditorCardProps) {
  const { policy, meta } = props;

  return (
    <Card
      title={<PolicySummary meta={meta} updatedAt={policy.updated_at} />}
    >
      <PolicyFields policy={policy} meta={meta} />
    </Card>
  );
}
