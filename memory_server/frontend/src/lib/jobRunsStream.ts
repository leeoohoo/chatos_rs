import type { JobRun } from '../types';

type JobRunsStreamFilters = {
  jobType?: string;
  status?: string;
};

type JobRunsStreamSnapshotMessage = {
  items?: JobRun[];
};

type JobRunsStreamUpsertMessage = {
  action?: string;
  job_run?: JobRun | null;
};

type JobRunsStreamHandlers = {
  onSnapshot: (items: JobRun[]) => void;
  onUpsert: (item: JobRun) => void;
  onResync?: () => void;
  onError?: (error: Error) => void;
};

const baseURL = import.meta.env.VITE_MEMORY_API_BASE ?? 'http://localhost:7080/api/memory/v1';

const buildStreamUrl = (filters: JobRunsStreamFilters): string => {
  const url = new URL(`${baseURL.replace(/\/$/, '')}/jobs/runs/stream`);
  if (filters.jobType && filters.jobType !== 'all') {
    url.searchParams.set('job_type', filters.jobType);
  }
  if (filters.status && filters.status !== 'all') {
    url.searchParams.set('status', filters.status);
  }
  url.searchParams.set('limit', '500');
  return url.toString();
};

const safeParse = <T,>(value: string): T | null => {
  try {
    return JSON.parse(value) as T;
  } catch {
    return null;
  }
};

export const connectJobRunsStream = (
  filters: JobRunsStreamFilters,
  handlers: JobRunsStreamHandlers,
): (() => void) => {
  const authToken = localStorage.getItem('memory_auth_token');
  const controller = new AbortController();

  const run = async () => {
    try {
      const response = await fetch(buildStreamUrl(filters), {
        method: 'GET',
        headers: authToken ? { Authorization: `Bearer ${authToken}` } : {},
        signal: controller.signal,
      });
      if (!response.ok) {
        throw new Error(`stream request failed: ${response.status}`);
      }
      if (!response.body) {
        throw new Error('stream response body missing');
      }

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      while (!controller.signal.aborted) {
        const { value, done } = await reader.read();
        if (done) {
          break;
        }
        buffer += decoder.decode(value, { stream: true });

        let boundary = buffer.indexOf('\n\n');
        while (boundary >= 0) {
          const rawEvent = buffer.slice(0, boundary);
          buffer = buffer.slice(boundary + 2);
          boundary = buffer.indexOf('\n\n');

          const lines = rawEvent.split('\n');
          let eventName = 'message';
          const dataLines: string[] = [];
          for (const line of lines) {
            if (line.startsWith('event:')) {
              eventName = line.slice(6).trim();
            } else if (line.startsWith('data:')) {
              dataLines.push(line.slice(5).trimStart());
            }
          }
          const data = dataLines.join('\n');
          if (!data) {
            continue;
          }

          if (eventName === 'snapshot') {
            const payload = safeParse<JobRunsStreamSnapshotMessage>(data);
            handlers.onSnapshot(Array.isArray(payload?.items) ? payload.items : []);
            continue;
          }
          if (eventName === 'upsert') {
            const payload = safeParse<JobRunsStreamUpsertMessage>(data);
            if (payload?.job_run) {
              handlers.onUpsert(payload.job_run);
            }
            continue;
          }
          if (eventName === 'resync') {
            handlers.onResync?.();
          }
        }
      }
    } catch (error) {
      if (controller.signal.aborted) {
        return;
      }
      handlers.onError?.(error instanceof Error ? error : new Error('job runs stream failed'));
    }
  };

  void run();

  return () => {
    controller.abort();
  };
};
