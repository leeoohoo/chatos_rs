import {
  Alert,
  Button,
  Card,
  Space,
  Table,
} from 'antd';

import { useI18n } from '../i18n';
import { AgentAiCreateModal } from './agentsPage/AgentAiCreateModal';
import { AgentConversationsDrawer } from './agentsPage/AgentConversationsDrawer';
import { AgentEditorModal } from './agentsPage/AgentEditorModal';
import { PluginPreviewModal } from './agentsPage/PluginPreviewModal';
import { SkillPreviewModal } from './agentsPage/SkillPreviewModal';
import { useAgentTableColumns } from './agentsPage/useAgentTableColumns';
import { useAgentsPageController } from './agentsPage/useAgentsPageController';

interface AgentsPageProps {
  filterUserId?: string;
  currentUserId: string;
  isAdmin: boolean;
}

export function AgentsPage({ filterUserId, currentUserId, isAdmin }: AgentsPageProps) {
  const { t } = useI18n();
  const controller = useAgentsPageController({
    filterUserId,
    currentUserId,
    isAdmin,
    t,
  });

  const columns = useAgentTableColumns({
    t,
    saving: controller.saving,
    isReadonlyForScope: controller.isReadonlyForScope,
    resolvePluginDisplayName: controller.resolvePluginDisplayName,
    resolveSkillDisplayName: controller.resolveSkillDisplayName,
    resolveModelDisplayName: controller.resolveModelDisplayName,
    onOpenConversation: controller.openConversationDrawer,
    onOpenEdit: controller.openEdit,
    onOpenPluginPreview: controller.openPluginPreview,
    onOpenSkillPreview: controller.openSkillPreview,
    onRemoveAgent: controller.removeAgent,
  });

  return (
    <Card
      title={t('agents.title')}
      extra={
        <Space>
          <Button onClick={() => { void controller.refresh(); }} loading={controller.loading}>
            {t('common.refresh')}
          </Button>
          <Button onClick={controller.openAiCreate} disabled={controller.crossScopeReadonly}>
            {t('agents.aiCreate')}
          </Button>
          <Button type="primary" onClick={controller.openCreate} disabled={controller.crossScopeReadonly}>
            {t('agents.create')}
          </Button>
        </Space>
      }
    >
      {controller.showAdminTip && (
          <Alert type="info" showIcon message={t('agents.adminTip')} style={{ marginBottom: 12 }} />
      )}
      {controller.showSharedReadonlyTip && (
        <Alert
          type="info"
          showIcon
          message={t('agents.sharedReadonlyTip')}
          style={{ marginBottom: 12 }}
        />
      )}
      <Alert
        type="info"
        showIcon
        message={`${t('agents.scopeUser')}: ${controller.scopeUserId || '-'}`}
        style={{ marginBottom: 12 }}
      />
      {controller.error && (
        <Alert type="error" showIcon message={controller.error} style={{ marginBottom: 12 }} />
      )}
      <Table
        rowKey="id"
        loading={controller.loading}
        dataSource={controller.items}
        columns={columns}
        pagination={{ pageSize: 20, showSizeChanger: false }}
        onRow={(record) => ({
          onClick: () => {
            void controller.openConversationDrawer(record);
          },
        })}
      />
      <AgentConversationsDrawer
        t={t}
        state={controller.conversationState}
        onClose={controller.closeConversationDrawer}
        onSelectSession={controller.loadConversationMessages}
      />

      <AgentEditorModal
        t={t}
        open={controller.editorOpen}
        saving={controller.saving}
        editor={controller.editorState}
        modelOptions={controller.aiModelOptions}
        pluginOptions={controller.editorPluginOptions}
        skillOptions={controller.editorSkillOptions}
        onCancel={controller.closeEditor}
        onSave={controller.saveEditor}
        onChange={controller.updateEditor}
        mergePluginSourcesWithSkills={controller.mergePluginSourcesWithSkills}
      />

      <AgentAiCreateModal
        t={t}
        state={controller.aiCreateState}
        saving={controller.saving}
        modelOptions={controller.aiModelOptions}
        onCancel={controller.closeAiCreate}
        onSubmit={controller.runAiCreate}
        onChange={controller.updateAiCreate}
      />

      <PluginPreviewModal
        t={t}
        state={controller.pluginPreviewState}
        resolvePluginDisplayName={controller.resolvePluginDisplayName}
        onCancel={controller.closePluginPreview}
      />

      <SkillPreviewModal
        t={t}
        state={controller.skillPreviewState}
        onCancel={controller.closeSkillPreview}
      />
    </Card>
  );
}
