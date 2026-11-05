// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { AgentFiltersCard } from './data/AgentFiltersCard';
import { AgentWorkspace } from './data/AgentWorkspace';
import type { DataSectionProps } from './data/types';

export function DataSection(props: DataSectionProps) {
  return (
    <div className="engine-data-page">
      <AgentFiltersCard
        form={props.form}
        initialValues={props.initialValues}
        loading={props.loading}
        onApplyFilters={props.onApplyFilters}
        onResetFilters={props.onResetFilters}
        onReload={props.onReload}
      />
      <AgentWorkspace {...props} />
    </div>
  );
}
