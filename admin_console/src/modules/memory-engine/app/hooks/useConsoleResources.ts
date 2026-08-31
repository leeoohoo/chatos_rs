// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { App } from 'antd';
import { useEffect, useRef, useState } from 'react';

import { api } from '../../api';
import type { DashboardStats } from '../types';

const EMPTY_DASHBOARD_STATS: DashboardStats = {
  sources: 0,
  models: 0,
  policies: 0,
  running: 0,
  done: 0,
  failed: 0,
};

function summarizeJobStats(jobStats: Record<string, Record<string, number>>) {
  return {
    running: Object.values(jobStats).reduce((sum, stats) => sum + (stats.running ?? 0), 0),
    done: Object.values(jobStats).reduce((sum, stats) => sum + (stats.done ?? 0), 0),
    failed: Object.values(jobStats).reduce((sum, stats) => sum + (stats.failed ?? 0), 0),
  };
}

export function useConsoleResources(enabled = true) {
  const { message } = App.useApp();
  const [loading, setLoading] = useState(false);
  const [initialized, setInitialized] = useState(false);
  const [dashboardStats, setDashboardStats] = useState<DashboardStats>(EMPTY_DASHBOARD_STATS);
  const [dashboardJobStats, setDashboardJobStats] = useState<
    Record<string, Record<string, number>>
  >({});
  const overviewRequestIdRef = useRef(0);

  const loadDashboardOverview = async () => {
    if (!enabled) {
      setDashboardStats(EMPTY_DASHBOARD_STATS);
      setDashboardJobStats({});
      setInitialized(true);
      setLoading(false);
      return;
    }
    const requestId = overviewRequestIdRef.current + 1;
    overviewRequestIdRef.current = requestId;
    setLoading(true);
    try {
      const overview = await api.getDashboardOverview();
      if (overviewRequestIdRef.current !== requestId) {
        return;
      }
      setDashboardStats({
        sources: overview.source_count,
        models: overview.model_count,
        policies: overview.policy_count,
        ...summarizeJobStats(overview.job_stats),
      });
      setDashboardJobStats(overview.job_stats);
    } catch (error) {
      if (overviewRequestIdRef.current === requestId) {
        message.error(`加载控制台概览失败：${String(error)}`);
      }
    } finally {
      if (overviewRequestIdRef.current === requestId) {
        setLoading(false);
      }
    }
  };

  useEffect(() => {
    if (!enabled) {
      setInitialized(true);
      setLoading(false);
      setDashboardStats(EMPTY_DASHBOARD_STATS);
      setDashboardJobStats({});
      return;
    }

    const initialize = async () => {
      try {
        await loadDashboardOverview();
      } finally {
        setInitialized(true);
      }
    };

    void initialize();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled]);

  return {
    initialized,
    loading,
    dashboardStats,
    dashboardJobStats,
    loadDashboardOverview,
  };
}
