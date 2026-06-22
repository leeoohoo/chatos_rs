import { Form } from 'antd';
import { useRef, useState } from 'react';

import { api } from '../../api';
import type { EngineJobRun, JobRunQuery } from '../../types';
import { textOrUndefined } from '../utils';

const DEFAULT_RUN_FILTERS: JobRunQuery = {
  job_type: undefined,
  trigger_type: undefined,
  thread_id: undefined,
  status: undefined,
  tenant_id: '',
  source_id: undefined,
  limit: 200,
};

type RunManagementOptions = {
  onError?: (message: string) => void;
};

export function useRunManagement(options?: RunManagementOptions) {
  const [runsLoading, setRunsLoading] = useState(false);
  const [runFilters, setRunFilters] = useState<JobRunQuery>(DEFAULT_RUN_FILTERS);
  const [threadJobRuns, setThreadJobRuns] = useState<EngineJobRun[]>([]);
  const [schedulerJobRuns, setSchedulerJobRuns] = useState<EngineJobRun[]>([]);

  const [runFilterForm] = Form.useForm<JobRunQuery>();
  const runsRequestIdRef = useRef(0);

  const reportError = (message: string) => {
    options?.onError?.(message);
  };

  const applyRunSnapshot = (threadRuns: EngineJobRun[], schedulerRuns: EngineJobRun[]) => {
    setThreadJobRuns(threadRuns);
    setSchedulerJobRuns(schedulerRuns);
  };

  const loadRuns = async (filters?: JobRunQuery) => {
    const nextFilters = filters ?? runFilters;
    const requestId = runsRequestIdRef.current + 1;
    runsRequestIdRef.current = requestId;
    setRunsLoading(true);
    try {
      const { thread_runs: threadRuns, scheduler_runs: schedulerRuns } =
        await api.getJobRunsBundle(nextFilters);
      if (runsRequestIdRef.current !== requestId) {
        return { threadRuns: [], schedulerRuns: [] };
      }
      applyRunSnapshot(threadRuns, schedulerRuns);
      return { threadRuns, schedulerRuns };
    } catch (error) {
      if (runsRequestIdRef.current === requestId) {
        reportError(`加载任务运行失败：${String(error)}`);
      }
      return { threadRuns: [], schedulerRuns: [] };
    } finally {
      if (runsRequestIdRef.current === requestId) {
        setRunsLoading(false);
      }
    }
  };

  const handleApplyRunFilters = async () => {
    try {
      const values = await runFilterForm.validateFields();
      const nextFilters: JobRunQuery = {
        job_type: textOrUndefined(values.job_type),
        trigger_type: textOrUndefined(values.trigger_type),
        thread_id: textOrUndefined(values.thread_id),
        status: textOrUndefined(values.status),
        tenant_id: textOrUndefined(values.tenant_id),
        source_id: textOrUndefined(values.source_id),
        limit: values.limit ?? 200,
      };
      setRunFilters(nextFilters);
      await loadRuns(nextFilters);
    } catch (error) {
      const text = String(error);
      if (!text.includes('validate')) {
        reportError(`应用任务筛选失败：${text}`);
      }
    }
  };

  const handleResetRunFilters = async () => {
    runFilterForm.setFieldsValue(DEFAULT_RUN_FILTERS);
    setRunFilters(DEFAULT_RUN_FILTERS);
    await loadRuns(DEFAULT_RUN_FILTERS);
  };

  return {
    runsLoading,
    runFilters,
    threadJobRuns,
    schedulerJobRuns,
    runFilterForm,
    setThreadJobRuns,
    setSchedulerJobRuns,
    applyRunSnapshot,
    loadRuns,
    handleApplyRunFilters,
    handleResetRunFilters,
  };
}
