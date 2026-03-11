import React, { createContext, useContext, useMemo, useState } from 'react';

export type Lang = 'zh-CN' | 'en-US';

type Dict = Record<string, string>;

const ZH: Dict = {
  'app.title': 'Memory 服务台',
  'app.subtitle': '会话、消息、总结统一管理',
  'nav.dashboard': '总览',
  'nav.sessions': '会话',
  'nav.sessionDetail': '会话详情',
  'nav.summaryLevels': '总结层级',
  'nav.models': '模型配置',
  'nav.jobConfigs': '任务配置',
  'nav.jobRuns': '任务运行',

  'auth.loginTitle': '登录 Memory 服务',
  'auth.username': '用户名',
  'auth.password': '密码',
  'auth.login': '登录',
  'auth.logout': '退出登录',

  'top.userId': '用户 ID',
  'top.userFilter': '按用户筛选（管理员）',
  'top.serviceToken': '服务 Token',
  'top.saveToken': '保存 Token',
  'top.selectedSession': '当前会话',
  'top.none': '无',

  'lang.zh': '中文',
  'lang.en': 'English',

  'common.refresh': '刷新',
  'common.loading': '加载中...',
  'common.action': '操作',
  'common.create': '新建',
  'common.edit': '编辑',
  'common.delete': '删除',
  'common.cancel': '取消',
  'common.save': '保存',
  'common.confirm': '确认',
  'common.test': '测试',
  'common.enabled': '启用',
  'common.disabled': '禁用',
  'common.noData': '暂无数据',

  'dashboard.title': '任务统计',
  'dashboard.empty': '近 24 小时暂无任务数据',

  'sessions.title': '会话列表',
  'sessions.newTitle': '新会话标题',
  'sessions.create': '创建会话',
  'sessions.id': 'ID',
  'sessions.titleCol': '标题',
  'sessions.user': '用户',
  'sessions.status': '状态',
  'sessions.updatedAt': '更新时间',
  'sessions.needUserId': '请先填写用户 ID',
  'sessions.adminAllTip': '管理员未填写筛选用户时，会显示全部用户会话。',
  'sessions.created': '会话创建成功',

  'sessionDetail.title': '会话详情',
  'sessionDetail.pickFirst': '请先在会话页选择一个会话',
  'sessionDetail.addRole': '角色',
  'sessionDetail.addMessage': '消息内容',
  'sessionDetail.add': '添加消息',
  'sessionDetail.messages': '消息',
  'sessionDetail.summaries': '总结',
  'sessionDetail.context': '上下文预览',
  'sessionDetail.sessionLabel': '会话',
  'sessionDetail.noSummary': '[暂无总结]',
  'sessionDetail.sourceCount': '来源条数',
  'sessionDetail.sourceTokens': '来源 Tokens',
  'sessionDetail.createdAt': '创建时间',

  'summaryLevels.title': '总结层级与图谱',
  'summaryLevels.pickFirst': '请先选择会话',
  'summaryLevels.level': '层级',
  'summaryLevels.total': '总数',
  'summaryLevels.pending': '待汇总',
  'summaryLevels.summarized': '已汇总',
  'summaryLevels.nodes': '图节点',
  'summaryLevels.edges': '图边',
  'summaryLevels.status': '状态',
  'summaryLevels.rollup': '汇总状态',
  'summaryLevels.parent': '父总结',
  'summaryLevels.excerpt': '摘要片段',
  'summaryLevels.from': '来源',
  'summaryLevels.to': '目标',
  'summaryLevels.sessionLabel': '会话',

  'models.title': '模型配置管理',
  'models.add': '新增模型',
  'models.edit': '编辑模型',
  'models.name': '配置名称',
  'models.provider': '供应商',
  'models.model': '模型名称',
  'models.baseUrl': 'Base URL',
  'models.apiKey': 'API Key',
  'models.thinking': '思考等级',
  'models.temperature': '温度',
  'models.supportImages': '支持图片',
  'models.supportReasoning': '支持推理',
  'models.supportResponses': '支持 Responses',
  'models.capabilities': '能力',
  'models.created': '创建时间',
  'models.updated': '更新时间',
  'models.required': '配置名称 / 模型名称为必填',
  'models.createSuccess': '模型配置创建成功',
  'models.updateSuccess': '模型配置更新成功',
  'models.deleteSuccess': '模型配置删除成功',
  'models.testOk': '连通性测试通过',
  'models.testFailed': '连通性测试失败',
  'models.deleteConfirm': '确认删除该模型配置？',

  'jobConfigs.title': '总结任务配置',
  'jobConfigs.runSummaryNow': '立即执行总结',
  'jobConfigs.runRollupNow': '立即执行再总结',
  'jobConfigs.summaryConfig': '一级总结任务',
  'jobConfigs.rollupConfig': '多级再总结任务',
  'jobConfigs.modelConfigId': '模型配置 ID',
  'jobConfigs.roundLimit': '批次条数',
  'jobConfigs.tokenLimit': 'Token 上限',
  'jobConfigs.targetTokens': '目标摘要长度',
  'jobConfigs.interval': '执行间隔(秒)',
  'jobConfigs.keepRaw': '保留原始 L0 数量',
  'jobConfigs.maxLevel': '最大层级',
  'jobConfigs.maxSessions': '单次会话上限',
  'jobConfigs.saved': '保存成功',

  'jobRuns.title': '任务运行记录',
  'jobRuns.jobType': '任务类型',
  'jobRuns.session': '会话',
  'jobRuns.status': '状态',
  'jobRuns.input': '输入',
  'jobRuns.output': '输出',
  'jobRuns.startedAt': '开始时间',
  'jobRuns.error': '错误信息',
};

const EN: Dict = {
  'app.title': 'Memory Console',
  'app.subtitle': 'Unified sessions, messages and summaries',
  'nav.dashboard': 'Overview',
  'nav.sessions': 'Sessions',
  'nav.sessionDetail': 'Session Detail',
  'nav.summaryLevels': 'Summary Levels',
  'nav.models': 'Model Configs',
  'nav.jobConfigs': 'Job Configs',
  'nav.jobRuns': 'Job Runs',

  'auth.loginTitle': 'Sign In to Memory',
  'auth.username': 'Username',
  'auth.password': 'Password',
  'auth.login': 'Sign In',
  'auth.logout': 'Sign Out',

  'top.userId': 'User ID',
  'top.userFilter': 'Filter by user (admin)',
  'top.serviceToken': 'Service Token',
  'top.saveToken': 'Save Token',
  'top.selectedSession': 'Selected Session',
  'top.none': 'None',

  'lang.zh': '中文',
  'lang.en': 'English',

  'common.refresh': 'Refresh',
  'common.loading': 'Loading...',
  'common.action': 'Actions',
  'common.create': 'Create',
  'common.edit': 'Edit',
  'common.delete': 'Delete',
  'common.cancel': 'Cancel',
  'common.save': 'Save',
  'common.confirm': 'Confirm',
  'common.test': 'Test',
  'common.enabled': 'Enabled',
  'common.disabled': 'Disabled',
  'common.noData': 'No data',

  'dashboard.title': 'Job Statistics',
  'dashboard.empty': 'No job data in the last 24 hours',

  'sessions.title': 'Sessions',
  'sessions.newTitle': 'New session title',
  'sessions.create': 'Create Session',
  'sessions.id': 'ID',
  'sessions.titleCol': 'Title',
  'sessions.user': 'User',
  'sessions.status': 'Status',
  'sessions.updatedAt': 'Updated At',
  'sessions.needUserId': 'Please input user ID first',
  'sessions.adminAllTip': 'When filter is empty, admin sees sessions from all users.',
  'sessions.created': 'Session created',

  'sessionDetail.title': 'Session Detail',
  'sessionDetail.pickFirst': 'Select a session first',
  'sessionDetail.addRole': 'Role',
  'sessionDetail.addMessage': 'Message content',
  'sessionDetail.add': 'Add Message',
  'sessionDetail.messages': 'Messages',
  'sessionDetail.summaries': 'Summaries',
  'sessionDetail.context': 'Context Preview',
  'sessionDetail.sessionLabel': 'Session',
  'sessionDetail.noSummary': '[no summary]',
  'sessionDetail.sourceCount': 'Source Count',
  'sessionDetail.sourceTokens': 'Source Tokens',
  'sessionDetail.createdAt': 'Created At',

  'summaryLevels.title': 'Summary Levels and Graph',
  'summaryLevels.pickFirst': 'Select a session first',
  'summaryLevels.level': 'Level',
  'summaryLevels.total': 'Total',
  'summaryLevels.pending': 'Pending',
  'summaryLevels.summarized': 'Summarized',
  'summaryLevels.nodes': 'Graph Nodes',
  'summaryLevels.edges': 'Graph Edges',
  'summaryLevels.status': 'Status',
  'summaryLevels.rollup': 'Rollup Status',
  'summaryLevels.parent': 'Parent Summary',
  'summaryLevels.excerpt': 'Summary Excerpt',
  'summaryLevels.from': 'From',
  'summaryLevels.to': 'To',
  'summaryLevels.sessionLabel': 'Session',

  'models.title': 'Model Config Management',
  'models.add': 'Add Model',
  'models.edit': 'Edit Model',
  'models.name': 'Config Name',
  'models.provider': 'Provider',
  'models.model': 'Model',
  'models.baseUrl': 'Base URL',
  'models.apiKey': 'API Key',
  'models.thinking': 'Thinking Level',
  'models.temperature': 'Temperature',
  'models.supportImages': 'Supports Images',
  'models.supportReasoning': 'Supports Reasoning',
  'models.supportResponses': 'Supports Responses',
  'models.capabilities': 'Capabilities',
  'models.created': 'Created At',
  'models.updated': 'Updated At',
  'models.required': 'Name / Model are required',
  'models.createSuccess': 'Model config created',
  'models.updateSuccess': 'Model config updated',
  'models.deleteSuccess': 'Model config deleted',
  'models.testOk': 'Connectivity test passed',
  'models.testFailed': 'Connectivity test failed',
  'models.deleteConfirm': 'Delete this model config?',

  'jobConfigs.title': 'Summary Job Configs',
  'jobConfigs.runSummaryNow': 'Run Summary Now',
  'jobConfigs.runRollupNow': 'Run Rollup Now',
  'jobConfigs.summaryConfig': 'L0 Summary Job',
  'jobConfigs.rollupConfig': 'Multi-level Rollup Job',
  'jobConfigs.modelConfigId': 'Model Config ID',
  'jobConfigs.roundLimit': 'Batch Size',
  'jobConfigs.tokenLimit': 'Token Limit',
  'jobConfigs.targetTokens': 'Target Summary Tokens',
  'jobConfigs.interval': 'Interval (sec)',
  'jobConfigs.keepRaw': 'Keep Raw L0 Count',
  'jobConfigs.maxLevel': 'Max Level',
  'jobConfigs.maxSessions': 'Max Sessions Per Tick',
  'jobConfigs.saved': 'Saved',

  'jobRuns.title': 'Job Runs',
  'jobRuns.jobType': 'Job Type',
  'jobRuns.session': 'Session',
  'jobRuns.status': 'Status',
  'jobRuns.input': 'Input',
  'jobRuns.output': 'Output',
  'jobRuns.startedAt': 'Started At',
  'jobRuns.error': 'Error',
};

type I18nValue = {
  lang: Lang;
  setLang: (lang: Lang) => void;
  t: (key: string) => string;
};

const I18nContext = createContext<I18nValue | null>(null);

export function I18nProvider({ children }: { children: React.ReactNode }) {
  const [lang, setLang] = useState<Lang>(() => {
    const saved = localStorage.getItem('memory_frontend_lang');
    return saved === 'en-US' ? 'en-US' : 'zh-CN';
  });

  const value = useMemo<I18nValue>(() => {
    const dict = lang === 'en-US' ? EN : ZH;
    return {
      lang,
      setLang: (next) => {
        localStorage.setItem('memory_frontend_lang', next);
        setLang(next);
      },
      t: (key: string) => dict[key] || key,
    };
  }, [lang]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n() {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    throw new Error('useI18n must be used inside I18nProvider');
  }
  return ctx;
}
