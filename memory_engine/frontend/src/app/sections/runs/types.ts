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
