import { useEffect, useMemo, useState } from 'react';
import { Alert, Button, Card, Space, Spin, Tabs, Typography } from 'antd';

import { useI18n } from '../i18n';
import { AgentMemoryConfigCard } from './jobConfigsPage/AgentMemoryConfigCard';
import { RollupConfigCard } from './jobConfigsPage/RollupConfigCard';
import { SummaryConfigCard } from './jobConfigsPage/SummaryConfigCard';
import { UserSelectorCard } from './jobConfigsPage/UserSelectorCard';
import {
  DEFAULT_AGENT_MEMORY_PROMPT_TEMPLATE,
  DEFAULT_ROLLUP_PROMPT_TEMPLATE,
  DEFAULT_SUMMARY_PROMPT_TEMPLATE,
  DEFAULT_TASK_EXECUTION_ROLLUP_PROMPT_TEMPLATE,
  DEFAULT_TASK_EXECUTION_SUMMARY_PROMPT_TEMPLATE,
} from './jobConfigsPage/helpers';
import { useJobConfigsPageData } from './jobConfigsPage/useJobConfigsPageData';

interface JobConfigsPageProps {
  userId: string;
  isAdmin: boolean;
  selectedSessionId?: string;
  onSelectUser?: (userId: string) => void;
  showUserSelector?: boolean;
}

export function JobConfigsPage({
  userId,
  isAdmin: _isAdmin,
  selectedSessionId,
  onSelectUser,
  showUserSelector = true,
}: JobConfigsPageProps) {
  const { t } = useI18n();
  const [activeTabKey, setActiveTabKey] = useState('summary');
  const {
    targetUserId,
    users,
    usersLoading,
    summaryCfg,
    rollupCfg,
    taskExecutionSummaryCfg,
    taskExecutionRollupCfg,
    agentMemoryCfg,
    modelOptions,
    loading,
    error,
    message,
    disabled,
    rollupTriggerHint,
    rollupKeepRawWarning,
    setTargetUserId,
    setSummaryCfg,
    setRollupCfg,
    setTaskExecutionSummaryCfg,
    setTaskExecutionRollupCfg,
    setAgentMemoryCfg,
    loadUsers,
    load,
    saveSummary,
    saveRollup,
    saveTaskExecutionSummary,
    saveTaskExecutionRollup,
    saveAgentMemory,
    runSummaryNow,
    runRollupNow,
    runTaskExecutionSummaryNow,
    runTaskExecutionRollupNow,
    runAgentMemoryNow,
    createEmptySummaryConfig,
    createEmptyRollupConfig,
    createEmptyTaskExecutionSummaryConfig,
    createEmptyTaskExecutionRollupConfig,
    createEmptyAgentMemoryConfig,
    setSummaryNumber,
    setRollupNumber,
    setTaskExecutionSummaryNumber,
    setTaskExecutionRollupNumber,
    setAgentMemoryNumber,
  } = useJobConfigsPageData({
    userId,
    selectedSessionId,
    showUserSelector,
    savedMessage: t('jobConfigs.saved'),
    rollupKeepRawHint: t('jobConfigs.rollupKeepRawHint'),
    rollupKeepRawWarning: t('jobConfigs.rollupKeepRawWarning'),
  });

  useEffect(() => {
    const uid = targetUserId.trim();
    if (uid) {
      onSelectUser?.(uid);
    }
  }, [onSelectUser, targetUserId]);

  const jobTabs = useMemo(() => ([
    {
      key: 'summary',
      label: t('jobConfigs.summaryConfig'),
      runLabel: t('jobConfigs.runSummaryNow'),
      onRun: () => void runSummaryNow(),
      description: '会话原始消息的一级总结配置。',
      content: (
        <SummaryConfigCard
          config={summaryCfg}
          modelOptions={modelOptions}
          title={t('jobConfigs.summaryConfig')}
          enabledLabel={t('common.enabled')}
          modelConfigIdLabel={t('jobConfigs.modelConfigId')}
          summaryPromptLabel={t('jobConfigs.summaryPrompt')}
          summaryPromptHint={t('jobConfigs.summaryPromptHint')}
          resetSummaryPromptLabel={t('jobConfigs.resetSummaryPrompt')}
          defaultSummaryPrompt={DEFAULT_SUMMARY_PROMPT_TEMPLATE}
          roundLimitLabel={t('jobConfigs.roundLimit')}
          tokenLimitLabel={t('jobConfigs.tokenLimit')}
          targetTokensLabel={t('jobConfigs.targetTokens')}
          intervalLabel={t('jobConfigs.interval')}
          maxCountLabel={t('jobConfigs.maxSessions')}
          maxCountKey="max_sessions_per_tick"
          saveLabel={t('common.save')}
          notConfiguredMessage={t('jobConfigs.notConfiguredSummary')}
          createConfigLabel={t('jobConfigs.createSummaryConfig')}
          onChange={setSummaryCfg}
          onSetNumber={setSummaryNumber}
          onSave={() => void saveSummary()}
          onCreate={createEmptySummaryConfig}
        />
      ),
    },
    {
      key: 'rollup',
      label: t('jobConfigs.rollupConfig'),
      runLabel: t('jobConfigs.runRollupNow'),
      onRun: () => void runRollupNow(),
      description: '会话多级汇总与 L0 保留策略配置。',
      content: (
        <RollupConfigCard
          config={rollupCfg}
          modelOptions={modelOptions}
          title={t('jobConfigs.rollupConfig')}
          enabledLabel={t('common.enabled')}
          modelConfigIdLabel={t('jobConfigs.modelConfigId')}
          summaryPromptLabel={t('jobConfigs.summaryPrompt')}
          summaryPromptHint={t('jobConfigs.summaryPromptHint')}
          resetSummaryPromptLabel={t('jobConfigs.resetSummaryPrompt')}
          defaultSummaryPrompt={DEFAULT_ROLLUP_PROMPT_TEMPLATE}
          roundLimitLabel={t('jobConfigs.roundLimit')}
          tokenLimitLabel={t('jobConfigs.tokenLimit')}
          targetTokensLabel={t('jobConfigs.targetTokens')}
          intervalLabel={t('jobConfigs.interval')}
          keepRawLabel={t('jobConfigs.keepRaw')}
          maxLevelLabel={t('jobConfigs.maxLevel')}
          maxCountLabel={t('jobConfigs.maxSessions')}
          maxCountKey="max_sessions_per_tick"
          saveLabel={t('common.save')}
          notConfiguredMessage={t('jobConfigs.notConfiguredRollup')}
          createConfigLabel={t('jobConfigs.createRollupConfig')}
          keepRawWarning={rollupKeepRawWarning}
          triggerHint={rollupTriggerHint}
          onChange={setRollupCfg}
          onSetNumber={setRollupNumber}
          onSave={() => void saveRollup()}
          onCreate={createEmptyRollupConfig}
        />
      ),
    },
    {
      key: 'task-summary',
      label: t('jobConfigs.taskExecutionSummaryConfig'),
      runLabel: t('jobConfigs.runTaskExecutionSummaryNow'),
      onRun: () => void runTaskExecutionSummaryNow(),
      description: '任务执行聊天记录的一级总结配置。',
      content: (
        <SummaryConfigCard
          config={taskExecutionSummaryCfg}
          modelOptions={modelOptions}
          title={t('jobConfigs.taskExecutionSummaryConfig')}
          enabledLabel={t('common.enabled')}
          modelConfigIdLabel={t('jobConfigs.modelConfigId')}
          summaryPromptLabel={t('jobConfigs.summaryPrompt')}
          summaryPromptHint={t('jobConfigs.summaryPromptHint')}
          resetSummaryPromptLabel={t('jobConfigs.resetSummaryPrompt')}
          defaultSummaryPrompt={DEFAULT_TASK_EXECUTION_SUMMARY_PROMPT_TEMPLATE}
          roundLimitLabel={t('jobConfigs.roundLimit')}
          tokenLimitLabel={t('jobConfigs.tokenLimit')}
          targetTokensLabel={t('jobConfigs.targetTokens')}
          intervalLabel={t('jobConfigs.interval')}
          maxCountLabel={t('jobConfigs.maxScopes')}
          maxCountKey="max_scopes_per_tick"
          saveLabel={t('common.save')}
          notConfiguredMessage={t('jobConfigs.notConfiguredTaskExecutionSummary')}
          createConfigLabel={t('jobConfigs.createTaskExecutionSummaryConfig')}
          onChange={(cfg) => setTaskExecutionSummaryCfg(cfg)}
          onSetNumber={(key, value, min) =>
            setTaskExecutionSummaryNumber(
              key as keyof NonNullable<typeof taskExecutionSummaryCfg>,
              value,
              min,
            )
          }
          onSave={() => void saveTaskExecutionSummary()}
          onCreate={createEmptyTaskExecutionSummaryConfig}
        />
      ),
    },
    {
      key: 'task-rollup',
      label: t('jobConfigs.taskExecutionRollupConfig'),
      runLabel: t('jobConfigs.runTaskExecutionRollupNow'),
      onRun: () => void runTaskExecutionRollupNow(),
      description: '任务执行总结的多级汇总配置。',
      content: (
        <RollupConfigCard
          config={taskExecutionRollupCfg}
          modelOptions={modelOptions}
          title={t('jobConfigs.taskExecutionRollupConfig')}
          enabledLabel={t('common.enabled')}
          modelConfigIdLabel={t('jobConfigs.modelConfigId')}
          summaryPromptLabel={t('jobConfigs.summaryPrompt')}
          summaryPromptHint={t('jobConfigs.summaryPromptHint')}
          resetSummaryPromptLabel={t('jobConfigs.resetSummaryPrompt')}
          defaultSummaryPrompt={DEFAULT_TASK_EXECUTION_ROLLUP_PROMPT_TEMPLATE}
          roundLimitLabel={t('jobConfigs.roundLimit')}
          tokenLimitLabel={t('jobConfigs.tokenLimit')}
          targetTokensLabel={t('jobConfigs.targetTokens')}
          intervalLabel={t('jobConfigs.interval')}
          keepRawLabel={t('jobConfigs.keepRaw')}
          maxLevelLabel={t('jobConfigs.maxLevel')}
          maxCountLabel={t('jobConfigs.maxScopes')}
          maxCountKey="max_scopes_per_tick"
          saveLabel={t('common.save')}
          notConfiguredMessage={t('jobConfigs.notConfiguredTaskExecutionRollup')}
          createConfigLabel={t('jobConfigs.createTaskExecutionRollupConfig')}
          keepRawWarning={null}
          triggerHint={null}
          onChange={(cfg) => setTaskExecutionRollupCfg(cfg)}
          onSetNumber={(key, value, min) =>
            setTaskExecutionRollupNumber(
              key as keyof NonNullable<typeof taskExecutionRollupCfg>,
              value,
              min,
            )
          }
          onSave={() => void saveTaskExecutionRollup()}
          onCreate={createEmptyTaskExecutionRollupConfig}
        />
      ),
    },
    {
      key: 'agent-memory',
      label: t('jobConfigs.agentMemoryConfig'),
      runLabel: t('jobConfigs.runAgentMemoryNow'),
      onRun: () => void runAgentMemoryNow(),
      description: '智能体长期记忆沉淀与项目维度汇总配置。',
      content: (
        <AgentMemoryConfigCard
          config={agentMemoryCfg}
          modelOptions={modelOptions}
          title={t('jobConfigs.agentMemoryConfig')}
          enabledLabel={t('common.enabled')}
          modelConfigIdLabel={t('jobConfigs.modelConfigId')}
          summaryPromptLabel={t('jobConfigs.summaryPrompt')}
          summaryPromptHint={t('jobConfigs.summaryPromptHint')}
          resetSummaryPromptLabel={t('jobConfigs.resetSummaryPrompt')}
          defaultSummaryPrompt={DEFAULT_AGENT_MEMORY_PROMPT_TEMPLATE}
          roundLimitLabel={t('jobConfigs.roundLimit')}
          tokenLimitLabel={t('jobConfigs.tokenLimit')}
          targetTokensLabel={t('jobConfigs.targetTokens')}
          intervalLabel={t('jobConfigs.interval')}
          keepRawLabel={t('jobConfigs.keepRaw')}
          maxLevelLabel={t('jobConfigs.maxLevel')}
          maxAgentsLabel={t('jobConfigs.maxAgents')}
          saveLabel={t('common.save')}
          notConfiguredMessage={t('jobConfigs.notConfiguredAgentMemory')}
          createConfigLabel={t('jobConfigs.createAgentMemoryConfig')}
          projectHintMessage={t('jobConfigs.agentMemoryProjectHint')}
          onChange={setAgentMemoryCfg}
          onSetNumber={setAgentMemoryNumber}
          onSave={() => void saveAgentMemory()}
          onCreate={createEmptyAgentMemoryConfig}
        />
      ),
    },
  ]), [
    agentMemoryCfg,
    createEmptyAgentMemoryConfig,
    createEmptyRollupConfig,
    createEmptySummaryConfig,
    createEmptyTaskExecutionRollupConfig,
    createEmptyTaskExecutionSummaryConfig,
    modelOptions,
    rollupCfg,
    rollupKeepRawWarning,
    rollupTriggerHint,
    runAgentMemoryNow,
    runRollupNow,
    runSummaryNow,
    runTaskExecutionRollupNow,
    runTaskExecutionSummaryNow,
    saveAgentMemory,
    saveRollup,
    saveSummary,
    saveTaskExecutionRollup,
    saveTaskExecutionSummary,
    setAgentMemoryCfg,
    setAgentMemoryNumber,
    setRollupCfg,
    setRollupNumber,
    setSummaryCfg,
    setSummaryNumber,
    setTaskExecutionRollupCfg,
    setTaskExecutionRollupNumber,
    setTaskExecutionSummaryCfg,
    setTaskExecutionSummaryNumber,
    summaryCfg,
    t,
    taskExecutionRollupCfg,
    taskExecutionSummaryCfg,
  ]);

  const activeTab = jobTabs.find((item) => item.key === activeTabKey) ?? jobTabs[0];

  return (
    <Space direction="vertical" size={12} style={{ width: '100%' }}>
      <Card
        title={t('jobConfigs.title')}
        extra={
          <Space>
            {showUserSelector ? (
              <Button onClick={() => void loadUsers()} loading={usersLoading}>
                {t('common.refresh')}
              </Button>
            ) : null}
            <Button onClick={() => void load()} loading={loading}>
              {t('common.refresh')}
            </Button>
            <Button type="primary" onClick={activeTab.onRun} disabled={disabled}>
              {activeTab.runLabel}
            </Button>
          </Space>
        }
      >
        {showUserSelector ? (
          <UserSelectorCard
            users={users}
            usersLoading={usersLoading}
            targetUserId={targetUserId}
            userIdTitle={t('top.userId')}
            roleTitle={t('top.role')}
            actionTitle={t('common.action')}
            title={t('jobConfigs.userListTitle')}
            viewConfigLabel={t('jobConfigs.viewConfig')}
            onSelectUser={(nextUserId) => {
              setTargetUserId(nextUserId);
              onSelectUser?.(nextUserId);
            }}
          />
        ) : null}

        {showUserSelector ? (
          <Alert
            type="info"
            showIcon
            message={`${t('jobConfigs.currentTarget')}: ${targetUserId || '-'}`}
            style={{ marginBottom: 12 }}
          />
        ) : null}
        {disabled ? (
          <Alert
            type="warning"
            showIcon
            message={t('sessions.needUserId')}
            style={{ marginBottom: 12 }}
          />
        ) : null}
        {error ? (
          <Alert type="error" showIcon message={error} style={{ marginBottom: 12 }} />
        ) : null}
        {message ? (
          <Alert type="success" showIcon message={message} style={{ marginBottom: 12 }} />
        ) : null}

        {loading &&
        !summaryCfg &&
        !rollupCfg &&
        !taskExecutionSummaryCfg &&
        !taskExecutionRollupCfg &&
        !agentMemoryCfg ? (
          <Spin />
        ) : (
          <Card
            styles={{
              body: {
                paddingTop: 16,
              },
            }}
          >
            <Space direction="vertical" size={16} style={{ width: '100%' }}>
              <div
                style={{
                  display: 'flex',
                  alignItems: 'flex-start',
                  gap: 12,
                  flexWrap: 'wrap',
                }}
              >
                <div>
                  <Typography.Title level={5} style={{ margin: 0 }}>
                    {activeTab.label}
                  </Typography.Title>
                  <Typography.Paragraph
                    type="secondary"
                    style={{ margin: '6px 0 0', maxWidth: 680 }}
                  >
                    {activeTab.description}
                  </Typography.Paragraph>
                </div>
              </div>

              <Tabs
                activeKey={activeTab.key}
                onChange={setActiveTabKey}
                items={jobTabs.map((item) => ({
                  key: item.key,
                  label: item.label,
                }))}
              />

              <div>{activeTab.content}</div>
            </Space>
          </Card>
        )}
      </Card>
    </Space>
  );
}
