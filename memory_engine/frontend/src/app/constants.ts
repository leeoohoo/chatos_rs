import type { JobTypeKey, PolicyMeta, PolicyViewKey } from './types';

export const RUN_STATUS_OPTIONS = ['running', 'done', 'failed', 'queued'];

export const JOB_TYPE_OPTIONS: JobTypeKey[] = [
  'summary',
  'rollup',
  'subject_memory',
  'thread_repair',
];

export const THINKING_LEVEL_OPTIONS = [
  { label: 'none', value: 'none' },
  { label: 'low', value: 'low' },
  { label: 'medium', value: 'medium' },
  { label: 'high', value: 'high' },
  { label: 'xhigh', value: 'xhigh' },
];

export const PIPELINE_POLICY_VIEWS: Array<{ key: PolicyViewKey; jobType: JobTypeKey }> = [
  { key: 'summary', jobType: 'summary' },
  { key: 'rollup', jobType: 'rollup' },
  { key: 'memory_from_summary', jobType: 'subject_memory' },
  { key: 'memory_rollup', jobType: 'subject_memory' },
  { key: 'thread_repair', jobType: 'thread_repair' },
];

export const PIPELINE_POLICY_META: Record<PolicyViewKey, PolicyMeta> = {
  summary: {
    tabLabel: '消息总结',
    title: '消息 -> 一级总结',
    tagColor: 'blue',
    description:
      '把线程里尚未总结的原始聊天消息压缩成第一层可复用总结，作为后续上下文构建和更高层聚合的基础材料。',
    inputText: '原始消息记录（message / record）',
    outputText: '线程一级总结（L0 summary）',
    purposeText: '先把长对话压缩成稳定摘要，减少直接读取原始消息的成本。',
    promptLabel: '消息总结 Prompt',
    promptPlaceholder:
      '为空时使用默认“消息总结”模板，重点保留已完成事项、当前进展、下一步计划，以及关键约束、风险、路径和用户要求。',
    tokenLimitLabel: '触发 Token 阈值',
    targetSummaryTokensLabel: '目标总结长度',
    intervalSecondsLabel: '调度间隔（秒）',
    maxThreadsPerTickLabel: '每轮处理线程数',
    showKeepLevel0: false,
    showMaxLevel: false,
  },
  rollup: {
    tabLabel: '总结再总结',
    title: '总结 -> 更高层总结',
    tagColor: 'geekblue',
    description:
      '把已经生成的线程总结继续向上压缩，形成更高层级的聚合总结，适合处理超长线程或长期会话。',
    inputText: '已完成的线程总结（summary）',
    outputText: '更高层级的聚合总结（rollup summary）',
    purposeText: '当总结本身也变多时，再做一轮压缩，进一步降低上下文体积。',
    promptLabel: '聚合总结 Prompt',
    promptPlaceholder:
      '为空时使用默认“总结再总结”模板，重点沉淀项目整体全貌、常用技能、环境信息、目录结构、接口边界和长期有效经验。',
    tokenLimitLabel: '聚合 Token 阈值',
    targetSummaryTokensLabel: '目标聚合长度',
    intervalSecondsLabel: '调度间隔（秒）',
    maxThreadsPerTickLabel: '每轮处理线程数',
    countLimitLabel: '聚合条数阈值',
    keepLevel0Label: '保留多少份 L0 总结',
    maxLevelLabel: '最大聚合层级',
    showKeepLevel0: true,
    showMaxLevel: true,
  },
  memory_from_summary: {
    tabLabel: '总结生成记忆',
    title: '总结 -> 一级记忆',
    tagColor: 'purple',
    description:
      '把已经生成的线程总结提炼成可长期复用的主题记忆，让平台后续构建上下文时优先召回稳定、长期有效的信息。',
    inputText: '已完成的线程总结（summary）',
    outputText: '一级主题记忆（L0 subject memory）',
    purposeText: '把阶段性总结沉淀成长期记忆，减少每次都从历史总结里重新翻找。',
    promptLabel: '记忆提炼 Prompt',
    promptPlaceholder:
      '为空时使用默认“总结生成记忆”模板，重点沉淀用户画像、协作习惯、长期偏好，以及智能体可复用的常识积累。',
    tokenLimitLabel: '记忆提炼 Token 阈值',
    targetSummaryTokensLabel: '目标记忆长度',
    intervalSecondsLabel: '调度间隔（秒）',
    maxThreadsPerTickLabel: '每轮处理主题数',
    countLimitLabel: '记忆条数阈值',
    keepLevel0Label: '底层记忆保留数量',
    maxLevelLabel: '最大记忆层级',
    sharedPolicyHint:
      '当前这两页在底层共用同一套 subject_memory 任务；除了各自的 Prompt 以外，其它调度参数属于同一套配置，保存公共参数时会同时生效。',
    showKeepLevel0: true,
    showMaxLevel: true,
  },
  memory_rollup: {
    tabLabel: '记忆再总结',
    title: '记忆 -> 更高层记忆',
    tagColor: 'magenta',
    description:
      '当主题记忆继续增多时，再把记忆继续向上归并成更高层记忆，保持长期上下文可控而且足够浓缩。',
    inputText: '已生成的主题记忆（subject memory）',
    outputText: '更高层主题记忆（rolled-up memory）',
    purposeText: '持续压缩长期记忆体积，避免记忆层本身再次膨胀。',
    promptLabel: '记忆归并 Prompt',
    promptPlaceholder:
      '为空时使用默认“记忆再总结”模板，重点归纳智能体在长期协作中逐渐形成的人格、性格、表达风格和判断方式。',
    tokenLimitLabel: '记忆归并 Token 阈值',
    targetSummaryTokensLabel: '目标高层记忆长度',
    intervalSecondsLabel: '调度间隔（秒）',
    maxThreadsPerTickLabel: '每轮处理主题数',
    countLimitLabel: '记忆条数阈值',
    keepLevel0Label: '底层记忆保留数量',
    maxLevelLabel: '最大记忆层级',
    sharedPolicyHint:
      '当前这两页在底层共用同一套 subject_memory 任务；除了各自的 Prompt 以外，其它调度参数属于同一套配置，保存公共参数时会同时生效。',
    showKeepLevel0: true,
    showMaxLevel: true,
  },
  thread_repair: {
    tabLabel: '修复总结',
    title: '原始消息 -> 修复总结',
    tagColor: 'cyan',
    description:
      '业务系统主动请求 repair summary 时，把线程中尚未总结的原始消息生成一份纠偏用总结，帮助下一轮上下文回到用户真实要求。',
    inputText: '线程内待总结的原始消息记录（pending unsummarized records）',
    outputText: '线程修复总结（thread repair summary）',
    purposeText:
      '只在接口主动触发时执行，用于修复上下文漂移；Token 配置控制单次 AI 分块上限，默认 200000。',
    promptLabel: '修复总结 Prompt',
    promptPlaceholder:
      '为空时使用默认“修复总结”模板，重点以用户消息为准，指出错误或未验证内容，并保留下轮必须遵守的约束。',
    tokenLimitLabel: '单次分块 Token 上限',
    targetSummaryTokensLabel: '目标修复总结长度',
    showTargetSummaryTokens: false,
    intervalSecondsLabel: '状态刷新间隔（秒）',
    showMaxThreadsPerTick: false,
    showKeepLevel0: false,
    showMaxLevel: false,
  },
};

export const THREAD_TABLE_SCROLL_Y = '100%';
export const RECORD_TABLE_SCROLL_Y = '100%';
export const DETAIL_TABLE_SCROLL_Y = '100%';

export const JOB_TRIGGER_LABELS: Record<string, string> = {
  thread_direct: '线程直接触发',
  scheduler: '系统调度',
};
