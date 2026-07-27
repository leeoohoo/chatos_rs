// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { ToolFamily } from '../lib/tools/catalog';
import type { UiLocale } from './messages';

type RendererSourceKind = 'builtin' | 'structured';

const normalize = (value: string): string => (
  value
    .trim()
    .toLowerCase()
    .replace(/[_-]+/g, ' ')
    .replace(/\s+/g, ' ')
);

const zhTitleMap: Record<string, string> = {
  'Input payload': '输入内容',
  'Input items': '输入项',
  'Input summary': '输入摘要',
  'Patch payload': '补丁内容',
  'Result payload': '结果内容',
  'Result items': '结果项',
  'Result summary': '结果摘要',
  'Command status': '命令状态',
  Output: '输出',
  'Terminal summary': '终端摘要',
  'Recent terminals': '最近终端',
  'Process summary': '进程摘要',
  'Process state': '进程状态',
  Processes: '进程列表',
  'Recent logs': '最近日志',
  'Log window': '日志窗口',
  'Process log': '进程日志',
  'Wait result': '等待结果',
  'Timeout note': '超时说明',
  'Input sent': '已发送输入',
  'Termination result': '终止结果',
  'Process details': '进程详情',
  'Page state': '页面状态',
  'Browser session': '浏览器会话',
  'Browser tabs': '浏览器标签页',
  'Console messages': '控制台消息',
  'JavaScript errors': 'JavaScript 错误',
  'JavaScript result': 'JavaScript 结果',
  'Vision analysis': '视觉分析',
  Images: '图片',
  'Inspection warning': '检查告警',
  'Research warning': '研究告警',
  'Selected URLs': '已选 URL',
  'Search hits': '搜索结果',
  'Extracted sources': '提取来源',
  Connections: '连接列表',
  'Remote entries': '远端条目',
  'Connection summary': '连接摘要',
  'Remote command': '远端命令',
  'Command output': '命令输出',
  'Remote directory': '远端目录',
  'Remote file': '远端文件',
  'Remote file content': '远端文件内容',
  'Connection target': '连接目标',
  'Connection result': '连接结果',
  Tags: '标签',
  'Notepad ready': '记事本已就绪',
  'Folder summary': '目录摘要',
  Folders: '目录列表',
  'Search result': '搜索结果',
  'Note summary': '笔记摘要',
  Notes: '笔记列表',
  'Tag summary': '标签摘要',
  Note: '笔记',
  'Note content': '笔记内容',
  'Saved note': '已保存笔记',
  'Folder result': '目录结果',
  'Folder moved': '目录迁移结果',
  'Delete result': '删除结果',
  'Notepad result': '记事本结果',
  'Available skills': '可用技能',
  'Embedded skills': '内嵌技能',
  Description: '描述',
  'Role definition': '角色定义',
  'Plugin sources': '插件来源',
  'Skill IDs': '技能 ID',
  'Default skill IDs': '默认技能 ID',
  'Recommended profile': '推荐配置',
  'Suggested skills': '建议技能',
  'Skill catalog': '技能目录',
  'Creation result': '创建结果',
  'Update result': '更新结果',
  Agent: '智能体',
  'Context preview': '上下文预览',
  Preview: '预览',
  'Plugin commands': '插件命令',
  'Related skills': '关联技能',
  Command: '命令',
  'Command description': '命令说明',
  'Command content': '命令内容',
  Skill: '技能',
  'Skill description': '技能说明',
  'Skill content': '技能内容',
  Plugin: '插件',
  'Plugin description': '插件说明',
  'Plugin content': '插件内容',
  'Review result': '确认结果',
  'Task scope': '任务范围',
  Tasks: '任务列表',
  Task: '任务',
  'Completion result': '完成结果',
  'Form values': '表单值',
  'Choice result': '选择结果',
  'Mixed form result': '混合表单结果',
  'Form result': '表单结果',
  'Chosen options': '已选项',
  'Chosen option': '已选项',
  Selection: '选择内容',
  'File content': '文件内容',
  'Directory entries': '目录条目',
  Matches: '匹配结果',
  'Touched files': '涉及文件',
  'Diff preview': 'Diff 预览',
  Message: '消息',
  Hint: '提示',
  'Research findings': '研究结论',
  'Page findings': '页面观察',
  'Web findings': '网页发现',
  'Source highlights': '重点来源',
  'Recommended next steps': '建议下一步',
  'Research overview': '研究概览',
  'Current page': '当前页面',
  'Extract summary': '提取摘要',
  'Console summary': '控制台摘要',
  'Network summary': '网络摘要',
  'Navigation timing': '导航计时',
  'Network resources': '网络资源',
  'Network requests': '网络请求',
  'Network request detail': '网络请求详情',
  'Request headers': '请求 Header',
  'Response headers': '响应 Header',
  'Request body': '请求 Body',
  'Response body': '响应 Body',
  'HAR capture': 'HAR 捕获',
  'Sanitized HAR export': '脱敏 HAR 导出',
  'WebSocket observation': 'WebSocket 观察',
  'WebSocket frames': 'WebSocket 帧',
  'Browser interception rules': '浏览器拦截规则',
  'CDP developer command': 'CDP 开发人员命令',
  'Downloaded file': '已下载文件',
  'Uploaded files': '已上传文件',
};

const zhLabelMap: Record<string, string> = {
  path: '路径',
  background: '后台执行',
  busy: '忙碌中',
  'reused terminal': '复用终端',
  'finished by': '结束方式',
  truncated: '已截断',
  scope: '范围',
  terminals: '终端数',
  status: '状态',
  active: '活动中',
  sockets: '连接数',
  'open sockets': '开放连接数',
  'total frames': '总帧数',
  'returned frames': '返回帧数',
  'dropped frames': '丢弃帧数',
  'protocol errors': '协议错误数',
  'text payloads included': '包含文本载荷',
  'binary payloads included': '包含二进制载荷',
  'route id': '规则 ID',
  pattern: '匹配模式',
  action: '动作',
  'body bytes': 'Body 字节数',
  'body sha256': 'Body SHA-256',
  'expires at': '过期时间',
  'route count': '规则数',
  target: '目标',
  method: '方法',
  'params bytes': '参数字节数',
  'params sha256': '参数 SHA-256',
  'tab id': '标签页 ID',
  'target id': '目标 ID',
  'target kind': '目标类型',
  'character count': '字符数',
  'text sha256': '文本 SHA-256',
  quality: '画质',
  'snapshot required': '需要重新快照',
  'option label': '选项文字',
  'option index': '选项序号',
  'delta x': '横向滚动量',
  'delta y': '纵向滚动量',
  'scroll x': '横向位置',
  'scroll y': '纵向位置',
  direction: '方向',
  'result bytes': '结果字节数',
  'returned logs': '返回日志数',
  'has more': '还有更多',
  showing: '当前窗口',
  'total lines': '总行数',
  completed: '已完成',
  'timed out': '已超时',
  'waited ms': '等待毫秒',
  'exit code': '退出码',
  submit: '已提交',
  'written chars': '写入字符数',
  'already exited': '已提前退出',
  killed: '已终止',
  'busy before': '终止前忙碌',
  'busy after': '终止后忙碌',
  state: '状态',
  title: '标题',
  url: 'URL',
  warning: '告警',
  preview: '预览',
  count: '数量',
  connection: '连接',
  host: '主机',
  command: '命令',
  'timeout seconds': '超时秒数',
  'output chars': '输出字符数',
  entries: '条目数',
  'source size': '源文件大小',
  name: '名称',
  port: '端口',
  username: '用户名',
  success: '成功',
  'remote host': '远端主机',
  'connected at': '连接时间',
  initialized: '已初始化',
  notes: '笔记数',
  version: '版本',
  folder: '目录',
  file: '文件',
  'updated at': '更新时间',
  tags: '标签',
  'deleted notes': '删除笔记数',
  from: '从',
  to: '到',
  'moved notes': '迁移笔记数',
  deleted: '已删除',
  'note id': '笔记 ID',
  category: '分类',
  enabled: '已启用',
  'plugin sources': '插件来源数',
  'embedded skills': '内嵌技能数',
  'skill ids': '技能 ID 数',
  'default skill ids': '默认技能 ID 数',
  created: '已创建',
  updated: '已更新',
  'role chars': '角色字符数',
  skills: '技能数',
  ref: '引用',
  'plugin source': '插件来源',
  'source path': '来源路径',
  'argument hint': '参数提示',
  'source type': '来源类型',
  source: '来源',
  repository: '仓库',
  branch: '分支',
  commands: '命令数',
  confirmed: '已确认',
  cancelled: '已取消',
  'created count': '创建数量',
  reason: '原因',
  'current turn only': '仅当前轮次',
  value: '值',
  'base url': 'Base URL',
  'api key': 'API Key',
  provider: '供应商',
  'model name': '模型名称',
  'thinking level': '思考等级',
  'start line': '起始行',
  'end line': '结束行',
  'line count': '总行数',
  'size bytes': '字节数',
  sha256: 'SHA-256',
  'chunk count': '分块数',
  'mime type': 'MIME 类型',
  'source kind': '来源类型',
  'source url': '来源 URL',
  type: '类型',
  page: '页面',
  'console messages': '控制台消息',
  'js errors': 'JS 错误',
  pages: '页数',
  'truncated pages': '截断页数',
  messages: '消息数',
  errors: '错误数',
  processes: '进程数',
  'search results': '搜索结果数',
  'extracted pages': '提取页数',
  'selected urls': '已选 URL 数',
  resources: '资源数',
  requests: '请求数',
  'request id': '请求 ID',
  returned: '返回数量',
  omitted: '省略数量',
  'duration ms': '耗时毫秒',
  'transfer bytes': '传输字节数',
  'overwrote existing': '覆盖已有文件',
  'exported entries': '导出条目数',
  'raw capture deleted': '原始捕获已删除',
  'request bodies included': '包含请求 Body',
  'response bodies included': '包含响应 Body',
};

const zhValueMap: Record<string, string> = {
  yes: '是',
  no: '否',
  unknown: '未知',
  'n/a': '暂无',
  dir: '目录',
  file: '文件',
  log: '日志',
  warn: '警告',
  warning: '警告',
  error: '错误',
  pending: '待处理',
  waiting: '等待中',
  running: '运行中',
  completed: '已完成',
  success: '成功',
  failed: '失败',
  cancelled: '已取消',
  canceled: '已取消',
  submitted: '已提交',
  doing: '进行中',
  todo: '待办',
  done: '已完成',
  high: '高',
  medium: '中',
  low: '低',
  root: '根目录',
  stdio: '标准输入输出',
  http: 'HTTP',
  ok: '成功',
  unavailable: '不可用',
  'in progress': '进行中',
};

const toolFamilyLabels: Record<ToolFamily, Record<UiLocale, string>> = {
  browser: {
    'zh-CN': '浏览器自动化',
    'en-US': 'Browser',
  },
  web: {
    'zh-CN': '网页研究',
    'en-US': 'Web research',
  },
  code: {
    'zh-CN': '文件与代码',
    'en-US': 'Code & files',
  },
  process: {
    'zh-CN': '进程控制',
    'en-US': 'Process',
  },
  remote: {
    'zh-CN': '远程连接',
    'en-US': 'Remote',
  },
  notepad: {
    'zh-CN': '笔记工具',
    'en-US': 'Notepad',
  },
  task: {
    'zh-CN': '任务管理',
    'en-US': 'Tasks',
  },
  agent: {
    'zh-CN': '智能体构建',
    'en-US': 'Agent builder',
  },
  memory: {
    'zh-CN': '记忆读取',
    'en-US': 'Memory',
  },
  generic: {
    'zh-CN': '工具调用',
    'en-US': 'Tool call',
  },
};

const toolFamilyDescriptions: Record<ToolFamily, Record<UiLocale, string>> = {
  browser: {
    'zh-CN': '浏览器自动化执行、页面观察与结果整理',
    'en-US': 'Browser automation, page inspection, and result summaries',
  },
  web: {
    'zh-CN': '网页搜索、提取与研究结果整理',
    'en-US': 'Web search, extraction, and research summaries',
  },
  code: {
    'zh-CN': '文件读取、搜索、修改与变更摘要',
    'en-US': 'File reads, search, edits, and change summaries',
  },
  process: {
    'zh-CN': '命令执行、终端状态与日志输出',
    'en-US': 'Command execution, terminal state, and log output',
  },
  remote: {
    'zh-CN': '远端连接、目录浏览与命令结果',
    'en-US': 'Remote connections, directory browsing, and command results',
  },
  notepad: {
    'zh-CN': '笔记、目录、标签与检索结果',
    'en-US': 'Notes, folders, tags, and search results',
  },
  task: {
    'zh-CN': '任务创建、列表与状态变更结果',
    'en-US': 'Task creation, lists, and status updates',
  },
  agent: {
    'zh-CN': '智能体建议、技能列表与上下文预览',
    'en-US': 'Agent recommendations, skill lists, and context previews',
  },
  memory: {
    'zh-CN': '命令、插件与技能详情内容',
    'en-US': 'Command, plugin, and skill details',
  },
  generic: {
    'zh-CN': '工具输入、输出与运行状态',
    'en-US': 'Tool input, output, and runtime state',
  },
};

const rendererSourceLabels: Record<RendererSourceKind, Record<UiLocale, string>> = {
  builtin: {
    'zh-CN': '内置面板',
    'en-US': 'Built-in panel',
  },
  structured: {
    'zh-CN': '通用面板',
    'en-US': 'Structured panel',
  },
};

const pickLocalized = (
  locale: UiLocale,
  source: string,
  zhMap: Record<string, string>,
): string => {
  if (locale !== 'zh-CN') {
    return source;
  }

  return zhMap[source] || zhMap[normalize(source)] || source;
};

export const translateToolTitle = (title: string, locale: UiLocale): string => (
  pickLocalized(locale, title, zhTitleMap)
);

export const translateToolLabel = (label: string, locale: UiLocale): string => (
  pickLocalized(locale, label, zhLabelMap)
);

export const translateToolValue = (value: string, locale: UiLocale): string => (
  pickLocalized(locale, value, zhValueMap)
);

export const formatToolPrimitive = (
  value: string | number | boolean | null,
  locale: UiLocale,
): string => {
  if (typeof value === 'boolean') {
    return locale === 'zh-CN'
      ? (value ? '是' : '否')
      : (value ? 'yes' : 'no');
  }

  if (value === null) {
    return 'null';
  }

  if (typeof value === 'string') {
    return translateToolValue(value, locale);
  }

  return String(value);
};

export const formatToolLineRangeLabel = (
  startLine: number | null,
  endLine: number | null,
  locale: UiLocale,
): string => {
  if (startLine !== null && endLine !== null) {
    return `${startLine}-${endLine}`;
  }
  if (startLine !== null) {
    return locale === 'zh-CN' ? `从 ${startLine}` : `from ${startLine}`;
  }
  if (endLine !== null) {
    return locale === 'zh-CN' ? `到 ${endLine}` : `to ${endLine}`;
  }
  return '';
};

export const getToolFamilyLabel = (family: ToolFamily, locale: UiLocale): string => (
  toolFamilyLabels[family]?.[locale] || toolFamilyLabels.generic[locale]
);

export const getToolFamilyDescription = (family: ToolFamily, locale: UiLocale): string => (
  toolFamilyDescriptions[family]?.[locale] || toolFamilyDescriptions.generic[locale]
);

export const getToolRendererSourceLabel = (
  kind: RendererSourceKind,
  locale: UiLocale,
): string => (
  rendererSourceLabels[kind][locale]
);
