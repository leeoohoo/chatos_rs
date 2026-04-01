import { useEffect, useMemo, useState } from 'react';

import { api } from '../../api/client';
import type {
  AgentMemoryJobConfig,
  RollupJobConfig,
  SummaryJobConfig,
  UserItem,
} from '../../types';
import {
  createAgentMemoryConfig,
  createRollupConfig,
  createSummaryConfig,
  normalizeMinInteger,
} from './helpers';

interface UseJobConfigsPageDataOptions {
  userId: string;
  selectedSessionId?: string;
  showUserSelector: boolean;
  savedMessage: string;
  rollupKeepRawHint: string;
  rollupKeepRawWarning: string;
}

export interface JobConfigsPageDataResult {
  targetUserId: string;
  users: UserItem[];
  usersLoading: boolean;
  summaryCfg: SummaryJobConfig | null;
  rollupCfg: RollupJobConfig | null;
  agentMemoryCfg: AgentMemoryJobConfig | null;
  modelOptions: Array<{ label: string; value: string }>;
  loading: boolean;
  error: string | null;
  message: string | null;
  disabled: boolean;
  rollupTriggerHint: string | null;
  rollupKeepRawWarning: string | null;
  setTargetUserId: (userId: string) => void;
  setSummaryCfg: (cfg: SummaryJobConfig | null) => void;
  setRollupCfg: (cfg: RollupJobConfig | null) => void;
  setAgentMemoryCfg: (cfg: AgentMemoryJobConfig | null) => void;
  loadUsers: () => Promise<void>;
  load: () => Promise<void>;
  saveSummary: () => Promise<void>;
  saveRollup: () => Promise<void>;
  saveAgentMemory: () => Promise<void>;
  runSummaryNow: () => Promise<void>;
  runRollupNow: () => Promise<void>;
  runAgentMemoryNow: () => Promise<void>;
  createEmptySummaryConfig: () => void;
  createEmptyRollupConfig: () => void;
  createEmptyAgentMemoryConfig: () => void;
  setSummaryNumber: (key: keyof SummaryJobConfig, value: number | null, min: number) => void;
  setRollupNumber: (key: keyof RollupJobConfig, value: number | null, min: number) => void;
  setAgentMemoryNumber: (
    key: keyof AgentMemoryJobConfig,
    value: number | null,
    min: number,
  ) => void;
}

export function useJobConfigsPageData({
  userId,
  selectedSessionId,
  showUserSelector,
  savedMessage,
  rollupKeepRawHint,
  rollupKeepRawWarning,
}: UseJobConfigsPageDataOptions): JobConfigsPageDataResult {
  const [targetUserId, setTargetUserId] = useState(userId);
  const [users, setUsers] = useState<UserItem[]>([]);
  const [usersLoading, setUsersLoading] = useState(false);
  const [summaryCfg, setSummaryCfg] = useState<SummaryJobConfig | null>(null);
  const [rollupCfg, setRollupCfg] = useState<RollupJobConfig | null>(null);
  const [agentMemoryCfg, setAgentMemoryCfg] = useState<AgentMemoryJobConfig | null>(null);
  const [modelOptions, setModelOptions] = useState<Array<{ label: string; value: string }>>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    setTargetUserId(userId);
  }, [userId]);

  const disabled = useMemo(() => !targetUserId.trim(), [targetUserId]);

  const rollupTriggerHintValue = useMemo(() => {
    if (!rollupCfg) {
      return null;
    }
    const keep = Math.max(0, rollupCfg.keep_raw_level0_count ?? 0);
    const round = Math.max(1, rollupCfg.round_limit ?? 1);
    if (keep <= 0) {
      return null;
    }
    return `${rollupKeepRawHint} ${keep} + ${round} = ${keep + round}`;
  }, [rollupCfg, rollupKeepRawHint]);

  const rollupKeepRawWarningValue = useMemo(() => {
    if (!rollupCfg) {
      return null;
    }
    const keep = Math.max(0, rollupCfg.keep_raw_level0_count ?? 0);
    const round = Math.max(1, rollupCfg.round_limit ?? 1);
    if (keep < round) {
      return null;
    }
    return rollupKeepRawWarning;
  }, [rollupCfg, rollupKeepRawWarning]);

  const loadUsers = async () => {
    setUsersLoading(true);
    try {
      const items = await api.listUsers(500);
      setUsers(items);
      if (items.length === 0) {
        return;
      }
      const currentTarget = targetUserId.trim();
      if (currentTarget && items.some((item) => item.username === currentTarget)) {
        return;
      }
      const preferred = userId.trim();
      if (preferred && items.some((item) => item.username === preferred)) {
        setTargetUserId(preferred);
        return;
      }
      setTargetUserId(items[0].username);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setUsersLoading(false);
    }
  };

  const load = async () => {
    if (disabled) {
      setSummaryCfg(null);
      setRollupCfg(null);
      setAgentMemoryCfg(null);
      setModelOptions([]);
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const uid = targetUserId.trim();
      const [summary, rollup, agentMemory, models] = await Promise.all([
        api.getSummaryJobConfig(uid),
        api.getRollupJobConfig(uid),
        api.getAgentMemoryJobConfig(uid),
        api.listModelConfigs(uid),
      ]);
      setSummaryCfg(summary ? { ...createSummaryConfig(uid), ...summary } : null);
      setRollupCfg(rollup ? { ...createRollupConfig(uid), ...rollup } : null);
      setAgentMemoryCfg(
        agentMemory
          ? { ...createAgentMemoryConfig(uid), ...agentMemory }
          : null,
      );
      setModelOptions(
        models.map((item) => ({
          label: `${item.name} (${item.provider}/${item.model})`,
          value: item.id,
        })),
      );
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!showUserSelector) {
      return;
    }
    void loadUsers();
  }, [showUserSelector]);

  useEffect(() => {
    void load();
  }, [targetUserId]);

  const saveSummary = async () => {
    if (!summaryCfg) {
      return;
    }
    setError(null);
    setMessage(null);
    try {
      const saved = await api.saveSummaryJobConfig({
        ...summaryCfg,
        user_id: targetUserId.trim(),
      });
      setSummaryCfg(saved);
      setMessage(savedMessage);
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const saveRollup = async () => {
    if (!rollupCfg) {
      return;
    }
    setError(null);
    setMessage(null);
    try {
      const saved = await api.saveRollupJobConfig({
        ...rollupCfg,
        user_id: targetUserId.trim(),
      });
      setRollupCfg(saved);
      setMessage(savedMessage);
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const saveAgentMemory = async () => {
    if (!agentMemoryCfg) {
      return;
    }
    setError(null);
    setMessage(null);
    try {
      const saved = await api.saveAgentMemoryJobConfig({
        ...agentMemoryCfg,
        user_id: targetUserId.trim(),
      });
      setAgentMemoryCfg(saved);
      setMessage(savedMessage);
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const runSummaryNow = async () => {
    setError(null);
    setMessage(null);
    try {
      const data = await api.runSummaryOnce(targetUserId.trim(), selectedSessionId);
      setMessage(JSON.stringify(data));
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const runRollupNow = async () => {
    setError(null);
    setMessage(null);
    try {
      const data = await api.runRollupOnce(targetUserId.trim());
      setMessage(JSON.stringify(data));
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const runAgentMemoryNow = async () => {
    setError(null);
    setMessage(null);
    try {
      const data = await api.runAgentMemoryOnce(targetUserId.trim());
      setMessage(JSON.stringify(data));
    } catch (err) {
      setError((err as Error).message);
    }
  };

  const createEmptySummaryConfig = () => {
    const uid = targetUserId.trim();
    if (!uid) {
      return;
    }
    setSummaryCfg(createSummaryConfig(uid));
  };

  const createEmptyRollupConfig = () => {
    const uid = targetUserId.trim();
    if (!uid) {
      return;
    }
    setRollupCfg(createRollupConfig(uid));
  };

  const createEmptyAgentMemoryConfig = () => {
    const uid = targetUserId.trim();
    if (!uid) {
      return;
    }
    setAgentMemoryCfg(createAgentMemoryConfig(uid));
  };

  const setSummaryNumber = (key: keyof SummaryJobConfig, value: number | null, min: number) => {
    setSummaryCfg((prev) => {
      if (!prev) {
        return prev;
      }
      return { ...prev, [key]: normalizeMinInteger(value, min) };
    });
  };

  const setRollupNumber = (key: keyof RollupJobConfig, value: number | null, min: number) => {
    setRollupCfg((prev) => {
      if (!prev) {
        return prev;
      }
      return { ...prev, [key]: normalizeMinInteger(value, min) };
    });
  };

  const setAgentMemoryNumber = (
    key: keyof AgentMemoryJobConfig,
    value: number | null,
    min: number,
  ) => {
    setAgentMemoryCfg((prev) => {
      if (!prev) {
        return prev;
      }
      return { ...prev, [key]: normalizeMinInteger(value, min) };
    });
  };

  return {
    targetUserId,
    users,
    usersLoading,
    summaryCfg,
    rollupCfg,
    agentMemoryCfg,
    modelOptions,
    loading,
    error,
    message,
    disabled,
    rollupTriggerHint: rollupTriggerHintValue,
    rollupKeepRawWarning: rollupKeepRawWarningValue,
    setTargetUserId,
    setSummaryCfg,
    setRollupCfg,
    setAgentMemoryCfg,
    loadUsers,
    load,
    saveSummary,
    saveRollup,
    saveAgentMemory,
    runSummaryNow,
    runRollupNow,
    runAgentMemoryNow,
    createEmptySummaryConfig,
    createEmptyRollupConfig,
    createEmptyAgentMemoryConfig,
    setSummaryNumber,
    setRollupNumber,
    setAgentMemoryNumber,
  };
}
