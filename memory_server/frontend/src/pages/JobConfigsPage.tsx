import { useEffect } from 'react';
import { Alert, Button, Card, Col, Row, Space, Spin } from 'antd';

import { useI18n } from '../i18n';
import { AgentMemoryConfigCard } from './jobConfigsPage/AgentMemoryConfigCard';
import { RollupConfigCard } from './jobConfigsPage/RollupConfigCard';
import { SummaryConfigCard } from './jobConfigsPage/SummaryConfigCard';
import { UserSelectorCard } from './jobConfigsPage/UserSelectorCard';
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
  const {
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
    rollupTriggerHint,
    rollupKeepRawWarning,
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
            <Button type="primary" onClick={() => void runSummaryNow()} disabled={disabled}>
              {t('jobConfigs.runSummaryNow')}
            </Button>
            <Button onClick={() => void runRollupNow()} disabled={disabled}>
              {t('jobConfigs.runRollupNow')}
            </Button>
            <Button onClick={() => void runAgentMemoryNow()} disabled={disabled}>
              {t('jobConfigs.runAgentMemoryNow')}
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

        {loading && !summaryCfg && !rollupCfg && !agentMemoryCfg ? (
          <Spin />
        ) : (
          <Row gutter={[12, 12]}>
            <Col xs={24} xl={8}>
              <SummaryConfigCard
                config={summaryCfg}
                modelOptions={modelOptions}
                title={t('jobConfigs.summaryConfig')}
                enabledLabel={t('common.enabled')}
                modelConfigIdLabel={t('jobConfigs.modelConfigId')}
                summaryPromptLabel={t('jobConfigs.summaryPrompt')}
                summaryPromptHint={t('jobConfigs.summaryPromptHint')}
                resetSummaryPromptLabel={t('jobConfigs.resetSummaryPrompt')}
                roundLimitLabel={t('jobConfigs.roundLimit')}
                tokenLimitLabel={t('jobConfigs.tokenLimit')}
                targetTokensLabel={t('jobConfigs.targetTokens')}
                intervalLabel={t('jobConfigs.interval')}
                maxSessionsLabel={t('jobConfigs.maxSessions')}
                saveLabel={t('common.save')}
                notConfiguredMessage={t('jobConfigs.notConfiguredSummary')}
                createConfigLabel={t('jobConfigs.createSummaryConfig')}
                onChange={setSummaryCfg}
                onSetNumber={setSummaryNumber}
                onSave={() => void saveSummary()}
                onCreate={createEmptySummaryConfig}
              />
            </Col>

            <Col xs={24} xl={8}>
              <RollupConfigCard
                config={rollupCfg}
                modelOptions={modelOptions}
                title={t('jobConfigs.rollupConfig')}
                enabledLabel={t('common.enabled')}
                modelConfigIdLabel={t('jobConfigs.modelConfigId')}
                summaryPromptLabel={t('jobConfigs.summaryPrompt')}
                summaryPromptHint={t('jobConfigs.summaryPromptHint')}
                resetSummaryPromptLabel={t('jobConfigs.resetSummaryPrompt')}
                roundLimitLabel={t('jobConfigs.roundLimit')}
                tokenLimitLabel={t('jobConfigs.tokenLimit')}
                targetTokensLabel={t('jobConfigs.targetTokens')}
                intervalLabel={t('jobConfigs.interval')}
                keepRawLabel={t('jobConfigs.keepRaw')}
                maxLevelLabel={t('jobConfigs.maxLevel')}
                maxSessionsLabel={t('jobConfigs.maxSessions')}
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
            </Col>

            <Col xs={24} xl={8}>
              <AgentMemoryConfigCard
                config={agentMemoryCfg}
                modelOptions={modelOptions}
                title={t('jobConfigs.agentMemoryConfig')}
                enabledLabel={t('common.enabled')}
                modelConfigIdLabel={t('jobConfigs.modelConfigId')}
                summaryPromptLabel={t('jobConfigs.summaryPrompt')}
                summaryPromptHint={t('jobConfigs.summaryPromptHint')}
                resetSummaryPromptLabel={t('jobConfigs.resetSummaryPrompt')}
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
            </Col>
          </Row>
        )}
      </Card>
    </Space>
  );
}
