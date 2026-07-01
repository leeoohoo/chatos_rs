// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FormInstance } from 'antd';

import type { EngineJobRun, JobRunQuery } from '../../../types';

export type RunsSectionProps = {
  form: FormInstance<JobRunQuery>;
  initialValues: JobRunQuery;
  loading: boolean;
  onApply: () => void;
  onReset: () => void;
  threadRuns: EngineJobRun[];
  schedulerRuns: EngineJobRun[];
};
